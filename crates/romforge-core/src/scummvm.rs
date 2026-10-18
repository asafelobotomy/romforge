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

use md5::{Digest as _, Md5};
use rusqlite::{Connection, params};

use crate::{Error, systems};

/// File name of the detection list in the databases folder.
pub const LIST_FILE: &str = "scummvm-detection.dat";

/// File name of libretro's `ScummVM.dat` (titles by game id) there.
pub const TITLES_FILE: &str = "scummvm-titles.dat";

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
    let mut out = Vec::new();
    for g in crate::dat::cmp_games(text)? {
        let mut files = Vec::new();
        let mut usable = true;
        for r in &g.roms {
            let Some(name) = r.get("name").filter(|n| !n.is_empty()) else {
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

/// MD5 of the first (or last) `bytes` of a file.
fn partial_md5(path: &Path, bytes: u64, tail: bool) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    if tail {
        let len = f.metadata()?.len();
        f.seek(SeekFrom::Start(len.saturating_sub(bytes)))?;
    }
    let mut h = Md5::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut left = bytes;
    while left > 0 {
        let want = left.min(buf.len() as u64) as usize;
        let n = f.read(&mut buf[..want])?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        left -= n as u64;
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// A folder ScummVM's list names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub dir: PathBuf,
    pub entry: usize,
    pub files: usize,
    pub size: u64,
}

/// Every folder under `roots` that is one of `entries`: all its listed
/// files there, by name (any case), size and partial MD5. Of several
/// entries that fit, the one listing the most files wins (as in ScummVM).
/// A folder inside a detected one isn't looked at again.
pub fn detect(entries: &[Entry], roots: &[PathBuf]) -> Vec<Found> {
    // Last name component -> (entry, file) listing it.
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, e) in entries.iter().enumerate() {
        for f in &e.files {
            let last = f.name.rsplit('/').next().unwrap_or(&f.name);
            by_name.entry(last).or_default().push(i);
        }
    }
    // Folder -> its files (relative path in lower case -> path).
    let mut dirs: BTreeMap<PathBuf, BTreeMap<String, PathBuf>> = BTreeMap::new();
    for root in roots {
        let walker = walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let n = e.file_name();
                n != crate::cleanup::TRASH_DIR && n != crate::home::STAGING
            });
        for e in walker.filter_map(Result::ok) {
            if !e.file_type().is_file() {
                continue;
            }
            // Files are filed under their folder and the one above it, for
            // entries naming a file in a subfolder ("data/x.dat").
            let mut rel = e.file_name().to_string_lossy().to_lowercase();
            let mut dir = e.path().parent();
            for _ in 0..2 {
                let Some(d) = dir else { break };
                if !d.starts_with(root) {
                    break;
                }
                dirs.entry(d.to_owned())
                    .or_default()
                    .insert(rel.clone(), e.path().to_owned());
                let Some(name) = d.file_name() else { break };
                rel = format!("{}/{rel}", name.to_string_lossy().to_lowercase());
                dir = d.parent();
            }
        }
    }
    let mut md5s: HashMap<(PathBuf, u64, bool), Option<String>> = HashMap::new();
    let mut out: Vec<Found> = Vec::new();
    for (dir, files) in &dirs {
        if out.iter().any(|f| dir.starts_with(&f.dir)) {
            continue;
        }
        let mut candidates: Vec<usize> = files
            .keys()
            .filter_map(|rel| by_name.get(rel.rsplit('/').next().unwrap_or(rel)))
            .flatten()
            .copied()
            .collect();
        candidates.sort_unstable();
        candidates.dedup();
        let mut best: Option<usize> = None;
        for i in candidates {
            let e = &entries[i];
            let fits = e.files.iter().all(|spec| {
                let Some(path) = files.get(&spec.name) else {
                    return false;
                };
                let Ok(meta) = std::fs::metadata(path) else {
                    return false;
                };
                if spec.size.is_some_and(|s| s != meta.len()) {
                    return false;
                }
                spec.md5.is_empty()
                    || md5s
                        .entry((path.clone(), spec.bytes, spec.tail))
                        .or_insert_with(|| partial_md5(path, spec.bytes, spec.tail).ok())
                        .as_deref()
                        == Some(spec.md5.as_str())
            });
            if fits && best.is_none_or(|b| entries[b].files.len() < e.files.len()) {
                best = Some(i);
            }
        }
        if let Some(entry) = best {
            let (mut n, mut size) = (0, 0);
            for f in walkdir::WalkDir::new(dir)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
            {
                if f.file_type().is_file() {
                    n += 1;
                    size += f.metadata().map_or(0, |m| m.len());
                }
            }
            out.push(Found {
                dir: dir.clone(),
                entry,
                files: n,
                size,
            });
        }
    }
    out
}

/// A folder's files as one SHA1: every path (lower case) and size, leaving
/// out what import adds (`romforge/`, `*.scummvm`).
pub fn fingerprint(dir: &Path) -> Option<String> {
    use sha1::{Digest as _, Sha1};
    let mut lines: Vec<String> = Vec::new();
    let walker = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !(e.depth() == 1 && e.file_name() == "romforge"));
    for e in walker {
        let e = e.ok()?;
        if !e.file_type().is_file() {
            continue;
        }
        let rel = e.path().strip_prefix(dir).ok()?.to_string_lossy().to_lowercase();
        if rel.ends_with(".scummvm") {
            continue;
        }
        lines.push(format!("{rel}\0{}", e.metadata().ok()?.len()));
    }
    if lines.is_empty() {
        return None;
    }
    lines.sort();
    let mut h = Sha1::new();
    for l in &lines {
        h.update(l.as_bytes());
        h.update(b"\n");
    }
    Some(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// What `scummvm --detect` printed for `root`: (folder, game id,
/// description). Its table is "engine:gameid  description  full path";
/// the path is found by the root it starts with.
pub fn parse_detect(out: &str, root: &Path) -> Vec<(PathBuf, String, String)> {
    let root = root.to_string_lossy();
    let mut found = Vec::new();
    for line in out.lines() {
        let Some((id, rest)) = line.split_once(' ') else {
            continue;
        };
        let Some((_, game)) = id.split_once(':') else {
            continue;
        };
        let Some(at) = rest
            .find(&format!(" {root}"))
            .or_else(|| rest.starts_with(&*root).then_some(0))
        else {
            continue;
        };
        let (desc, path) = rest.split_at(at);
        let path = path.trim_start();
        if game.is_empty() || !(path == root || path.starts_with(&format!("{root}/"))) {
            continue;
        }
        found.push((
            PathBuf::from(path.trim_end_matches('/')),
            game.to_owned(),
            desc.trim().to_owned(),
        ));
    }
    found
}

/// Keep what ScummVM's --detect named. Returns how many folders.
pub(crate) fn remember(conn: &Connection, found: &[(PathBuf, String, String)]) -> Result<usize, Error> {
    let mut n = 0;
    for (dir, code, title) in found {
        let Some(fp) = fingerprint(dir) else { continue };
        conn.execute(
            "INSERT INTO scummvm_ids (path, fingerprint, code, title) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET fingerprint = excluded.fingerprint, code = excluded.code,
               title = excluded.title",
            params![dir.to_string_lossy(), fp, code, title],
        )?;
        n += 1;
    }
    Ok(n)
}

/// The scanned folders, without those inside another.
pub(crate) fn top_roots(conn: &Connection) -> Result<Vec<PathBuf>, Error> {
    let roots: Vec<PathBuf> = conn
        .prepare("SELECT path FROM scan_roots ORDER BY length(path), path")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(PathBuf::from)
        .collect();
    let mut top: Vec<PathBuf> = Vec::new();
    for r in roots {
        if !top.iter().any(|t| r.starts_with(t)) && r.is_dir() {
            top.push(r);
        }
    }
    Ok(top)
}

/// Folders ScummVM's --detect named before, where they are now: at the
/// same place with the same files, or (copied by import, which writes a
/// `.scummvm` file) wherever a folder with that fingerprint has one.
fn asked_before(
    conn: &Connection,
    roots: &[PathBuf],
    named: &[PathBuf],
) -> Result<Vec<(PathBuf, String, String)>, Error> {
    let rows: Vec<(String, String, String, String)> = conn
        .prepare("SELECT path, fingerprint, code, title FROM scummvm_ids")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let free = |d: &Path| !named.iter().any(|n| d.starts_with(n));
    let mut out: Vec<(PathBuf, String, String)> = Vec::new();
    for (path, fp, code, title) in &rows {
        let p = PathBuf::from(path);
        if p.is_dir() && free(&p) && fingerprint(&p).as_ref() == Some(fp) {
            out.push((p, code.clone(), title.clone()));
        }
    }
    let by_fp: HashMap<&str, (&str, &str)> = rows
        .iter()
        .map(|(_, f, c, t)| (f.as_str(), (c.as_str(), t.as_str())))
        .collect();
    let mut with_id: Vec<PathBuf> = Vec::new();
    for root in roots {
        for e in walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if e.file_type().is_file()
                && e.file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .ends_with(".scummvm")
                && let Some(d) = e.path().parent()
                && !with_id.iter().any(|w| w == d)
            {
                with_id.push(d.to_owned());
            }
        }
    }
    for d in with_id {
        if !free(&d) || out.iter().any(|(p, _, _)| *p == d) {
            continue;
        }
        if let Some(fp) = fingerprint(&d)
            && let Some((code, title)) = by_fp.get(fp.as_str())
        {
            out.push((d.clone(), (*code).to_owned(), (*title).to_owned()));
            conn.execute(
                "INSERT OR REPLACE INTO scummvm_ids (path, fingerprint, code, title) VALUES (?1, ?2, ?3, ?4)",
                params![d.to_string_lossy(), fp, code, title],
            )?;
        }
    }
    Ok(out)
}

/// Work out the ScummVM games again from the list in `list` (when there
/// is one), what ScummVM's --detect named before, and every scanned
/// folder. Returns how many there are.
pub(crate) fn refresh(conn: &Connection, list: Option<&Path>) -> Result<usize, Error> {
    let entries = match list.filter(|p| p.is_file()) {
        Some(p) => parse(&String::from_utf8_lossy(&std::fs::read(p)?))?,
        None => Vec::new(),
    };
    // libretro's names, when its ScummVM.dat is kept beside the list.
    let names: HashMap<String, String> = list
        .and_then(Path::parent)
        .map(|d| d.join(TITLES_FILE))
        .filter(|p| p.is_file())
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| titles(&String::from_utf8_lossy(&b)).ok())
        .unwrap_or_default();
    let top = top_roots(conn)?;
    let found = if entries.is_empty() {
        Vec::new()
    } else {
        detect(&entries, &top)
    };
    let system = systems::by_key("ScummVM").map_or(0, |s| s.id);
    conn.execute("DELETE FROM folder_games", [])?;
    let mut insert = conn.prepare(
        "INSERT OR IGNORE INTO folder_games (path, system_id, title, code, files, size)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for f in &found {
        let e = &entries[f.entry];
        let title = match names.get(&e.code) {
            Some(n) => Entry {
                title: n.clone(),
                ..e.clone()
            }
            .row_title(),
            None => e.row_title(),
        };
        insert.execute(params![
            f.dir.to_string_lossy(),
            system,
            title,
            e.code,
            f.files,
            f.size
        ])?;
    }
    let named: Vec<PathBuf> = found.iter().map(|f| f.dir.clone()).collect();
    let asked = asked_before(conn, &top, &named)?;
    for (dir, code, title) in &asked {
        let (mut n, mut size) = (0, 0);
        for f in walkdir::WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if f.file_type().is_file() {
                n += 1;
                size += f.metadata().map_or(0, |m| m.len());
            }
        }
        let title = match (names.get(code), tag_of(title)) {
            (Some(n), Some(tag)) => format!("{n} {tag}"),
            (Some(n), None) => n.clone(),
            (None, _) => title.clone(),
        };
        insert.execute(params![dir.to_string_lossy(), system, title, code, n, size])?;
    }
    Ok(found.len() + asked.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"scummvm (
	author "scummvm"
	version "2.9.0"
)

game (
	name "sky"
	title "Beneath a Steel Sky"
	extra "CD"
	language "en"
	platform "pc"
	sourcefile "sky"
	engine "sky"
	rom ( name "SKY.DNR" size 80 md5-5000 0123456789abcdef0123456789abcdef )
	rom ( name "sky.dsk" size -1 md5-5000 t:0123456789abcdef0123456789abcdef )
)

game (
	name "macgame"
	title "A Mac Game"
	extra ""
	language "en"
	platform "macintosh"
	sourcefile "x"
	engine "x"
	rom ( name "Game" size-rd 100 md5-5000 r:0123456789abcdef0123456789abcdef )
)
"#;

    #[test]
    fn parses_scummvm_s_list() {
        let e = parse(LIST).unwrap();
        assert_eq!(e.len(), 1, "the resource-fork entry is skipped");
        assert_eq!(e[0].code, "sky");
        assert_eq!(e[0].row_title(), "Beneath a Steel Sky (CD) (pc) (en)");
        assert_eq!(e[0].files[0].name, "sky.dnr");
        assert_eq!(e[0].files[0].size, Some(80));
        assert_eq!(e[0].files[1].size, None);
        assert!(e[0].files[1].tail && e[0].files[1].bytes == 5000);
        assert!(parse("clrmamepro ( name \"x\" )").is_err());
    }

    #[test]
    fn partial_md5_of_head_and_tail() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("f");
        std::fs::write(&p, b"abcdef").unwrap();
        // md5("abc"), md5("def")
        assert_eq!(
            partial_md5(&p, 3, false).unwrap(),
            "900150983cd24fb0d6963f7d28e17f72"
        );
        assert_eq!(
            partial_md5(&p, 3, true).unwrap(),
            "4ed9407630eb1000c0f6b63842defa7d"
        );
        assert_eq!(
            partial_md5(&p, 5000, false).unwrap(),
            "e80b5017098950fc58aad83c8c14978e"
        );
    }
}
