//! Milestone 14: ScummVM games, which are folders of data files. They are
//! named by ScummVM's own detection list, which the installed ScummVM
//! writes on request (`--dump-all-detection-entries`, a clrmamepro-style
//! DAT). ScummVM's detection data is GPL: romforge never ships it; the
//! copy made on the owner's machine is kept in `Databases/`.
//!
//! An entry lists files by name, size and the MD5 of their first N bytes
//! (`md5-5000 <hex>`; `t:<hex>` for the last N). A folder is that game when
//! every listed file is in it with that size and MD5, as ScummVM itself
//! decides. That is the list's own evidence, not a DAT hash: such a game
//! is "detected", never verified (house rule 8). Entries that need a Mac
//! resource fork are skipped.
//!
//! The list leaves out the engines with their own detection code (SCUMM,
//! SCI, Sky, Glk): for those, ScummVM itself is asked (`--detect
//! --recursive`, once per scanned folder, when you read its list). What it
//! names is kept with a fingerprint of the folder's files, so the name
//! follows the folder when import copies it or undo brings it back.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use md5::{Digest as _, Md5};
use rusqlite::{Connection, params};

use crate::{Error, systems};

mod detect;
pub use detect::*;
mod store;
pub(crate) use store::*;

/// libretro's names for ScummVM game ids (its `ScummVM.dat`: one game per
/// id and variant; the first name of an id is kept). Its box art goes by
/// these names.
pub fn titles(text: &str) -> Result<HashMap<String, String>, Error> {
    let mut out = HashMap::new();
    for g in crate::dat::cmp_games(text)? {
        if let (Some(code), Some(name)) = (g.fields.get("code"), g.fields.get("name"))
            && !code.is_empty()
            && !name.is_empty()
        {
            out.entry(code.clone()).or_insert_with(|| name.clone());
        }
    }
    if out.is_empty() {
        return Err(Error::Dat("not libretro's ScummVM.dat (no game ids)".into()));
    }
    Ok(out)
}

/// The last `(...)` of a description ("... (VGA/DOS/English)"), which
/// tells versions apart.
fn tag_of(description: &str) -> Option<&str> {
    let d = description.trim_end();
    d.ends_with(')')
        .then(|| d.rfind(" (").map(|i| &d[i + 1..]))
        .flatten()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// ScummVM's game id ("sky", "queen").
    pub code: String,
    pub title: String,
    pub extra: String,
    pub language: String,
    pub platform: String,
    pub files: Vec<FileSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSpec {
    /// Path inside the game's folder, lower case, `/`-separated.
    pub name: String,
    /// `None`: any size.
    pub size: Option<u64>,
    /// Lower-case hex; empty: any content.
    pub md5: String,
    /// How many bytes the MD5 covers.
    pub bytes: u64,
    /// The MD5 is of the last `bytes`, not the first.
    pub tail: bool,
}

impl Entry {
    /// The library title: ScummVM's title, then what tells versions apart.
    pub fn row_title(&self) -> String {
        let mut t = self.title.trim().to_owned();
        if t.is_empty() {
            t = self.code.clone();
        }
        for part in [&self.extra, &self.platform, &self.language] {
            let p = part.trim();
            if !p.is_empty() && !p.eq_ignore_ascii_case("unk") {
                t += &format!(" ({p})");
            }
        }
        t
    }
}

/// Parse the detection list ScummVM writes.
pub fn parse(text: &str) -> Result<Vec<Entry>, Error> {
    // ScummVM escapes quotes and backslashes in its strings
    // (`extra "5.25\" Booter"`), which plain clrmamepro DATs never do.
    let text = text
        .replace("\\\\", "\u{0}")
        .replace("\\\"", "\u{201d}")
        .replace('\u{0}', "\\");
    // A file that only has to be there has no MD5 (`md5-0  )`).
    let text: String = text
        .lines()
        .map(|l| match l.rfind(" md5-") {
            Some(i)
                if l[i + 5..].split_whitespace().nth(1).is_none_or(|t| t == ")")
                    && l[i + 5..].contains(')') =>
            {
                let key_end = i + 5 + l[i + 5..].find(char::is_whitespace).unwrap_or(0);
                format!("{} \"\" {}\n", &l[..key_end], l[key_end..].trim_start())
            }
            _ => format!("{l}\n"),
        })
        .collect();
    let mut out = Vec::new();
    for g in crate::dat::cmp_games(&text)? {
        let mut files = Vec::new();
        let mut usable = true;
        for r in &g.roms {
            // `*`: any file (AGI booter disks); not something to look for.
            let Some(name) = r.get("name").filter(|n| !n.is_empty() && !n.contains('*')) else {
                usable = false;
                break;
            };
            let Some(size) = r.get("size") else {
                // `size-rd`: a Mac resource fork.
                usable = false;
                break;
            };
            let size = match size.parse::<i64>() {
                Ok(n) if n >= 0 => Some(n as u64),
                Ok(_) => None,
                Err(_) => {
                    usable = false;
                    break;
                }
            };
            let Some((key, value)) = r.iter().find(|(k, _)| k.starts_with("md5-")) else {
                usable = false;
                break;
            };
            let Ok(bytes) = key["md5-".len()..].parse::<u64>() else {
                usable = false;
                break;
            };
            let (tail, md5) = match value.split_once(':') {
                None => (false, value.as_str()),
                Some(("t", h)) => (true, h),
                // Resource or data forks, archives: not read here.
                Some(_) => {
                    usable = false;
                    break;
                }
            };
            let md5 = md5.to_ascii_lowercase();
            if !md5.is_empty() && !(md5.len() == 32 && md5.bytes().all(|b| b.is_ascii_hexdigit())) {
                usable = false;
                break;
            }
            files.push(FileSpec {
                name: name.replace('\\', "/").to_lowercase(),
                size,
                md5,
                bytes,
                tail,
            });
        }
        let code = g.fields.get("name").cloned().unwrap_or_default();
        if !usable || files.is_empty() || code.is_empty() {
            continue;
        }
        let field = |k: &str| g.fields.get(k).cloned().unwrap_or_default();
        out.push(Entry {
            title: field("title"),
            extra: field("extra"),
            language: field("language"),
            platform: field("platform"),
            code,
            files,
        });
    }
    if out.is_empty() {
        return Err(Error::Dat(
            "not ScummVM's detection list (no usable entries)".into(),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
