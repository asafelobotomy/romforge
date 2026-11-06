//! Milestone 27, stage 2: every save in `Saves/<system>/` as a plain list,
//! read-only. A save is matched to a game only by evidence, never by title
//! (house rule 8):
//! - by serial or title ID in its path (`BASLUS-00892` on a PS1 card's
//!   folder, `BLUS30443` for PS3, `ULUS10041` for PSP), when exactly one
//!   DAT title has that serial on the system;
//! - by the content's file name (`<content stem>.srm`), when exactly one
//!   game of the system has that stem (RetroArch, mGBA, melonDS, Mednafen).
//!
//! Anything else is "not matched".

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::dat::serial_key;

/// What a save was matched by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchedBy {
    Serial,
    FileName,
}

/// One save file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Save {
    /// Its folder in `Saves/` (a system key).
    pub system: String,
    /// Where it is below that folder.
    pub file: PathBuf,
    pub path: PathBuf,
    pub size: u64,
    /// Seconds since 1970 (0 when unknown).
    pub modified: u64,
    /// A serial or title ID its path holds, as written.
    pub serial: Option<String>,
    /// The game it belongs to, when matched.
    pub game: Option<String>,
    pub matched_by: Option<MatchedBy>,
}

/// Folders of a system's saves that are copies, not saves.
const NOT_SAVES: &str = "backup";

/// The saves under `saves` (`Saves/`), by system then path.
pub fn walk(saves: &Path) -> Vec<Save> {
    let mut out = Vec::new();
    let Ok(systems) = std::fs::read_dir(saves) else {
        return out;
    };
    let mut systems: Vec<_> = systems.filter_map(Result::ok).collect();
    systems.sort_by_key(|e| e.file_name());
    for entry in systems {
        if entry.path().is_dir()
            && let Some(system) = entry.file_name().to_str()
        {
            files_in(&entry.path(), Path::new(""), system, &mut out);
        }
    }
    out
}

fn files_in(dir: &Path, rel: &Path, system: &str, out: &mut Vec<Save>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name();
        let below = rel.join(&name);
        let Ok(meta) = e.metadata() else { continue };
        if meta.is_dir() {
            if rel.as_os_str().is_empty() && name == NOT_SAVES {
                continue;
            }
            files_in(&e.path(), &below, system, out);
        } else if meta.is_file() && !unfinished(&name.to_string_lossy()) {
            out.push(Save {
                system: system.to_owned(),
                serial: serial_in(&below.to_string_lossy()),
                file: below,
                path: e.path(),
                size: meta.len(),
                modified: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs()),
                game: None,
                matched_by: None,
            });
        }
    }
}

/// A file being written (`.part`, or romforge's `.<name>.romforge-part`).
fn unfinished(name: &str) -> bool {
    name.ends_with(".part") || name.ends_with(".romforge-part")
}

/// A serial in `text`: four letters, then (after at most one `-`, `_` or
/// space) five digits that no other digit follows, as `SLUS-00892`,
/// `BASLUS-00892`, `ULUS10041` or `BLUS30443-SAVE00` hold one. Returned as
/// letters then digits (`SLUS-00892`).
pub fn serial_in(text: &str) -> Option<String> {
    let b = text.as_bytes();
    for start in 0..b.len() {
        let letters = b.get(start..start + 4)?;
        if !letters.iter().all(u8::is_ascii_uppercase) {
            continue;
        }
        // The run of letters must end here: the serial's letters are the
        // last four of it (`BA` before `SLUS`).
        if b.get(start + 4).is_some_and(u8::is_ascii_uppercase) {
            continue;
        }
        let mut at = start + 4;
        if b.get(at).is_some_and(|c| matches!(c, b'-' | b'_' | b' ')) {
            at += 1;
        }
        let digits = b.get(at..at + 5);
        let more = b.get(at + 5).is_some_and(u8::is_ascii_digit);
        if digits.is_some_and(|d| d.iter().all(u8::is_ascii_digit)) && !more {
            return Some(format!("{}-{}", &text[start..start + 4], &text[at..at + 5]));
        }
    }
    None
}

/// What the library knows, to match saves against.
#[derive(Debug, Default)]
pub struct Known {
    /// (system key, [`serial_key`]) -> the DAT titles with that serial.
    pub serials: BTreeMap<(String, String), BTreeSet<String>>,
    /// (system key, file stem) -> the games whose file has that stem.
    pub stems: BTreeMap<(String, String), BTreeSet<String>>,
}

/// Fill in each save's game from `known`.
pub fn match_all(saves: &mut [Save], known: &Known) {
    for s in saves {
        let by_serial = s.serial.as_ref().and_then(|serial| {
            let titles = known.serials.get(&(s.system.clone(), serial_key(serial)))?;
            one(titles)
        });
        if let Some(title) = by_serial {
            s.game = Some(title);
            s.matched_by = Some(MatchedBy::Serial);
            continue;
        }
        let stem = s.file.file_stem().map(|n| n.to_string_lossy().into_owned());
        let by_name = stem
            .and_then(|st| known.stems.get(&(s.system.clone(), st)))
            .and_then(one);
        if let Some(title) = by_name {
            s.game = Some(title);
            s.matched_by = Some(MatchedBy::FileName);
        }
    }
}

/// The only title, or none when there are several.
fn one(titles: &BTreeSet<String>) -> Option<String> {
    (titles.len() == 1)
        .then(|| titles.iter().next().cloned())
        .flatten()
}

#[cfg(test)]
mod tests;
