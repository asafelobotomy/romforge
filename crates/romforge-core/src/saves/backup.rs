//! Milestone 27, stage 1: a backup copy of every save that has to stay
//! outside `Saves/` (RPCS3's, Cemu's, the 3DS emulators' on your own SD
//! card, Dolphin's Wii NAND: `launch::save_tree`), so it can be restored.
//!
//! Each game's save folder is copied to
//! `Saves/<system>/backup/<emulator>/<its path in the tree>/<when>/`, the
//! tree's layout kept, `<when>` a UTC time (`2026-10-25T15-00-00Z`). A copy
//! is made only when the folder differs from the last one (its files, their
//! sizes and SHA1s); the last [`KEEP`] copies of each are kept, older ones
//! removed. The original is only ever read. A copy is written beside its
//! place as `<when>.part` and renamed when whole.
//!
//! Made after a game romforge started ends (`Library::record_play`) and on
//! demand; never while the emulator runs (the caller checks). Restoring is
//! [`restore`]. xemu is the exception: its saves are inside its HDD image.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::Error;
use crate::home::Home;
use crate::launch::existing::UserDirs;
use crate::launch::{SaveTree, save_tree};

pub mod restore;

/// The copies kept of each save folder.
pub const KEEP: usize = 5;

/// One game's save folder and its copies.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Backed {
    /// Its path in the tree (`savedata/BLUS30443-SAVE00`).
    pub save: PathBuf,
    /// Where it is (`None`: gone from the tree, copies kept).
    pub source: Option<PathBuf>,
    /// Its copies, newest first, by `<when>`.
    pub versions: Vec<String>,
}

/// What [`run`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct BackedUp {
    /// Save folders copied (changed since their last copy).
    pub copied: usize,
    /// Unchanged, so not copied again.
    pub same: usize,
    /// Old copies removed (beyond [`KEEP`]).
    pub pruned: usize,
}

/// Whether `name` keeps saves outside `Saves/` that get backup copies.
pub fn backs_up(name: &str) -> bool {
    matches!(
        name,
        "rpcs3" | "cemu" | "azahar" | "lime3ds" | "citra" | "dolphin"
    )
}

/// Where `tree`'s copies of `name` go.
fn backup_dir(home: &Home, tree: &SaveTree, name: &str) -> PathBuf {
    home.saves().join(tree.system).join("backup").join(name)
}

/// `secs` since 1970 as `YYYY-MM-DDTHH-MM-SSZ` (a folder name, sorting by
/// time).
fn stamp(secs: u64) -> String {
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Howard Hinnant's days-to-civil.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}-{:02}-{:02}Z",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

/// Each file under `dir` (its path below it) → (size, SHA1).
fn contents(dir: &Path) -> Result<BTreeMap<PathBuf, (u64, String)>, Error> {
    let mut out = BTreeMap::new();
    for e in walkdir::WalkDir::new(dir).follow_links(false) {
        let e = e.map_err(std::io::Error::from)?;
        if !e.file_type().is_file() {
            continue;
        }
        let rel = e.path().strip_prefix(dir).unwrap_or(e.path()).to_path_buf();
        let size = e.metadata().map_err(std::io::Error::from)?.len();
        out.insert(rel, (size, sha1_of(e.path())?));
    }
    Ok(out)
}

fn sha1_of(path: &Path) -> std::io::Result<String> {
    use sha1::Digest;
    use std::io::Read;
    let (mut h, mut f) = (sha1::Sha1::new(), std::fs::File::open(path)?);
    let mut buf = vec![0; 1 << 16];
    loop {
        match f.read(&mut buf)? {
            0 => return Ok(crate::hash::hex(&h.finalize())),
            n => h.update(&buf[..n]),
        }
    }
}

/// A save folder's copies, newest first (finished ones only).
fn versions(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with('Z') && !n.ends_with(".part"))
        .collect();
    v.sort_by(|a, b| b.cmp(a));
    v
}

/// Copy every save folder of `name` that changed since its last copy.
/// `now`: seconds since 1970, naming the copies.
pub fn run(home: &Home, name: &str, user: Option<&UserDirs>, now: u64) -> Result<BackedUp, Error> {
    let mut done = BackedUp::default();
    let Some(tree) = save_tree(name, home, user) else {
        return Ok(done);
    };
    let to = backup_dir(home, &tree, name);
    let when = stamp(now);
    for save in tree.folders() {
        let source = tree.root.join(&save);
        let at = to.join(&save);
        let had = versions(&at);
        if had.first() == Some(&when) {
            done.same += 1;
            continue;
        }
        let now_has = contents(&source)?;
        if now_has.is_empty() {
            continue;
        }
        if let Some(last) = had.first()
            && contents(&at.join(last))? == now_has
        {
            done.same += 1;
            continue;
        }
        let part = at.join(format!("{when}.part"));
        let _ = std::fs::remove_dir_all(&part);
        for rel in now_has.keys() {
            let target = part.join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            crate::organize::copy_checked(&source.join(rel), &target)?;
        }
        if contents(&part)? != now_has {
            let _ = std::fs::remove_dir_all(&part);
            return Err(Error::Launch(format!(
                "{} changed while it was copied",
                source.display()
            )));
        }
        std::fs::rename(&part, at.join(&when))?;
        done.copied += 1;
        for old in versions(&at).iter().skip(KEEP) {
            std::fs::remove_dir_all(at.join(old))?;
            done.pruned += 1;
        }
    }
    Ok(done)
}

/// Every save folder of `name` that is in its tree or has copies.
pub fn list(home: &Home, name: &str, user: Option<&UserDirs>) -> Vec<Backed> {
    let Some(tree) = save_tree(name, home, user) else {
        return Vec::new();
    };
    let to = backup_dir(home, &tree, name);
    let mut saves: Vec<PathBuf> = tree.folders();
    // Copies of a save gone from the tree: the version folders' parents.
    for e in walkdir::WalkDir::new(&to)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
    {
        let n = e.file_name().to_string_lossy();
        if e.file_type().is_dir()
            && n.ends_with('Z')
            && let Some(save) = e.path().parent().and_then(|p| p.strip_prefix(&to).ok())
            && !saves.iter().any(|s| s == save)
        {
            saves.push(save.to_path_buf());
        }
    }
    saves.sort();
    saves
        .into_iter()
        .map(|save| Backed {
            source: Some(tree.root.join(&save)).filter(|s| s.is_dir()),
            versions: versions(&to.join(&save)),
            save,
        })
        .collect()
}

#[cfg(test)]
mod tests;
