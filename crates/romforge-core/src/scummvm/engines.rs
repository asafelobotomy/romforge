//! ScummVM games of the engines its list leaves out (SCUMM: LucasArts and
//! Humongous; SCI: Sierra; Beneath a Steel Sky), told by the files they
//! are made of: a folder or a zip or 7z ScummVM's --detect hasn't named
//! (it never looks inside an archive) is one game when it holds an
//! engine's own files. Only names and sizes are read, and for SCUMM's
//! index files their first 8 bytes (as streams; nothing is unpacked).
//!
//! This says which engine, not which game: the row is titled by the
//! folder or archive, or by the game id a SCUMM index file's name gives
//! (`tentacle.000`) when libretro's names know it. ScummVM's --detect
//! names the folder once cleanup has unpacked it.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::*;
use crate::scan::Stamp;

/// A folder or archive holding an engine's files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recognised {
    pub place: PathBuf,
    /// "scumm", "sci" or "sky".
    pub engine: &'static str,
    /// ScummVM's game id, when the files name it.
    pub code: Option<String>,
    pub files: usize,
    pub size: u64,
}

/// File name (lower case) -> size, of the files directly in one folder.
type Names = BTreeMap<String, u64>;

/// The SCUMM index files: `<id>.000` (v5, v6), `<id>.la0` (v7, v8),
/// `<id>.he0` (Humongous), each with the file that must be beside it.
const INDEXES: [(&str, &[&str]); 3] = [("000", &["001"]), ("la0", &["la1"]), ("he0", &["he1", "(a)"])];

/// Files other engines number the same way.
const NOT_SCUMM: [&str; 3] = ["resource", "resmap", "ressci"];

/// Whether 8 bytes start a SCUMM block: a four-letter tag and a big-endian
/// size, as they are or XORed with 0x69 (v5 and v6 files are).
fn scumm_block(head: &[u8; 8], size: u64) -> bool {
    [0u8, 0x69].iter().any(|&key| {
        let b: Vec<u8> = head.iter().map(|x| x ^ key).collect();
        let len = u32::from_be_bytes([b[4], b[5], b[6], b[7]]) as u64;
        b[..4]
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            && (8..=size).contains(&len)
    })
}

/// The engine (and game id, if the names give it) whose files `names`
/// are. `listed`: whether ScummVM's list names a file (then the list
/// decides). `head`: the first 8 bytes of a file.
fn engine_of(
    names: &Names,
    listed: &dyn Fn(&str) -> bool,
    head: &mut dyn FnMut(&str) -> Option<[u8; 8]>,
) -> Option<(&'static str, Option<String>)> {
    let has = |n: &str| names.contains_key(n) && !listed(n);
    if has("sky.dnr") && has("sky.dsk") {
        return Some(("sky", Some("sky".into())));
    }
    let sci16 = has("resource.map") && (has("resource.000") || has("resource.001"));
    let sci32 = (has("resmap.000") || has("resmap.001")) && (has("ressci.000") || has("ressci.001"));
    if sci16 || sci32 {
        return Some(("sci", None));
    }
    for (name, size) in names {
        let Some((stem, ext)) = name.rsplit_once('.') else {
            continue;
        };
        let Some((_, beside)) = INDEXES.iter().find(|(e, _)| *e == ext) else {
            continue;
        };
        if stem.is_empty()
            || NOT_SCUMM.contains(&stem)
            || !has(name)
            || !beside.iter().any(|b| has(&format!("{stem}.{b}")))
        {
            continue;
        }
        if head(name).is_some_and(|h| scumm_block(&h, *size)) {
            return Some(("scumm", Some(stem.to_owned())));
        }
    }
    // v1 to v4: numbered `.lfl` rooms (`00.lfl` or `000.lfl` the index).
    let lfl = |n: &str| {
        n.strip_suffix(".lfl")
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
    };
    if (has("00.lfl") || has("000.lfl"))
        && names
            .keys()
            .any(|n| has(n) && (n == "disk01.lec" || (lfl(n) && n != "00.lfl" && n != "000.lfl")))
    {
        return Some(("scumm", None));
    }
    None
}

/// First 8 bytes of a file.
fn head_of(path: &Path) -> Option<[u8; 8]> {
    let mut b = [0u8; 8];
    File::open(path).ok()?.read_exact(&mut b).ok()?;
    Some(b)
}

/// First 8 bytes of archive members read before, while the archive stays
/// the same.
static HEADS: Mutex<Option<HashMap<(PathBuf, String), Head>>> = Mutex::new(None);
type Head = (Stamp, Option<[u8; 8]>);

fn member_head(archive: &Path, stamp: Stamp, members: &super::packed::Members, m: usize) -> Option<[u8; 8]> {
    let key = (archive.to_owned(), members[m].0.clone());
    if let Some((s, h)) = HEADS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .get(&key)
        && *s == stamp
    {
        return *h;
    }
    let mut got = None;
    super::packed::read_members(archive, members, &BTreeSet::from([m]), &mut |_, r| {
        let mut b = [0u8; 8];
        got = r.read_exact(&mut b).ok().map(|()| b);
    });
    HEADS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(key, (stamp, got));
    got
}

/// What an archive holds, in any folder of it.
fn in_archive(archive: &Path, listed: &dyn Fn(&str) -> bool) -> Option<Recognised> {
    let (stamp, members) = super::packed::members(archive)?;
    // Folder inside the archive -> file name (lower case) -> member.
    let mut dirs: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for (i, (name, _)) in members.iter().enumerate() {
        let lower = name.replace('\\', "/").to_lowercase();
        let (dir, file) = lower.rsplit_once('/').unwrap_or(("", &lower));
        dirs.entry(dir.to_owned()).or_default().insert(file.to_owned(), i);
    }
    for files in dirs.values() {
        let names: Names = files.iter().map(|(n, &m)| (n.clone(), members[m].1)).collect();
        let mut head = |n: &str| member_head(archive, stamp, &members, files[n]);
        if let Some((engine, code)) = engine_of(&names, listed, &mut head) {
            return Some(Recognised {
                place: archive.to_owned(),
                engine,
                code,
                files: members.len(),
                size: members.iter().map(|(_, s)| s).sum(),
            });
        }
    }
    None
}

/// Every folder or archive under `roots` that holds an engine's files,
/// leaving out what is in `named` (named already) or inside another.
/// `listed`: the file names ScummVM's list names.
pub(crate) fn recognise(roots: &[PathBuf], listed: &HashSet<String>, named: &[PathBuf]) -> Vec<Recognised> {
    let listed = |n: &str| listed.contains(n);
    let mut dirs: BTreeMap<PathBuf, BTreeMap<String, (u64, PathBuf)>> = BTreeMap::new();
    let mut archives: Vec<PathBuf> = Vec::new();
    for root in roots {
        let walker = walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !crate::layout::is_own_dir(e.file_name()));
        for e in walker.filter_map(Result::ok) {
            if !e.file_type().is_file() {
                continue;
            }
            if super::is_archive(e.path()) {
                archives.push(e.path().to_owned());
            }
            if let (Some(dir), Ok(meta)) = (e.path().parent(), e.metadata()) {
                dirs.entry(dir.to_owned()).or_default().insert(
                    e.file_name().to_string_lossy().to_lowercase(),
                    (meta.len(), e.path().to_owned()),
                );
            }
        }
    }
    let mut out: Vec<Recognised> = Vec::new();
    let taken = |p: &Path, out: &[Recognised]| {
        named.iter().any(|n| p.starts_with(n)) || out.iter().any(|r| p.starts_with(&r.place))
    };
    for (dir, files) in &dirs {
        if taken(dir, &out) {
            continue;
        }
        let names: Names = files.iter().map(|(n, (s, _))| (n.clone(), *s)).collect();
        let mut head = |n: &str| head_of(&files[n].1);
        if let Some((engine, code)) = engine_of(&names, &listed, &mut head) {
            let (n, size) = folder_stats(dir);
            out.push(Recognised {
                place: dir.clone(),
                engine,
                code,
                files: n,
                size,
            });
        }
    }
    for a in archives {
        if !taken(&a, &out)
            && let Some(r) = in_archive(&a, &listed)
        {
            out.push(r);
        }
    }
    out
}

/// The file names (lower case) ScummVM's list names.
pub(crate) fn listed_names(entries: &[Entry]) -> HashSet<String> {
    entries
        .iter()
        .flat_map(|e| &e.files)
        .map(|f| f.name.rsplit('/').next().unwrap_or(&f.name).to_owned())
        .collect()
}

/// A recognised game's title: libretro's name for its game id, else the
/// folder's or archive's own name.
pub(crate) fn title_of(r: &Recognised, names: &HashMap<String, String>) -> (String, String) {
    let known = r.code.as_ref().filter(|c| names.contains_key(*c));
    let own = || {
        let p = if r.place.is_dir() {
            r.place.file_name()
        } else {
            r.place.file_stem()
        };
        p.map_or_else(|| r.engine.to_owned(), |n| n.to_string_lossy().into_owned())
    };
    match known {
        Some(code) => (names[code].clone(), code.clone()),
        // Beneath a Steel Sky's id is certain without libretro's names.
        None if r.engine == "sky" => (own(), "sky".into()),
        None => (own(), String::new()),
    }
}

#[cfg(test)]
mod tests;
