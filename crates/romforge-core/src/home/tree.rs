//! Moving whole folders across drives, never half-way.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::Error;
use crate::organize::{copy_checked, move_file, rename_noreplace, same_file, sync_dir};

/// Move the folder `from` to `to` (which must not exist) where a rename
/// can't (across drives): copied whole, empty folders too, into a staging
/// folder beside `to`, each file checked; renamed into place once complete;
/// only then is `from` removed. If the copy fails, the staging folder goes
/// and `from` is untouched (and `to` doesn't exist). If removing `from`
/// fails, `to` is complete and what is left of `from` stays: see
/// [`merge_tree`]. Returns the number of files.
pub(crate) fn move_tree(from: &Path, to: &Path) -> Result<usize, Error> {
    if to.symlink_metadata().is_ok() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} exists", to.display()),
        )));
    }
    let name = to.file_name().unwrap_or_default().to_string_lossy();
    let staging = to.with_file_name(crate::layout::part(&name));
    let copied = copy_tree(from, &staging).and_then(|copied| rename_noreplace(&staging, to).map(|()| copied));
    let copied = match copied {
        Ok(c) => c,
        Err(e) => {
            if staging.symlink_metadata().is_ok() {
                let _ = std::fs::remove_dir_all(&staging);
            }
            return Err(e.into());
        }
    };
    if let Some(dir) = to.parent() {
        sync_dir(dir);
    }
    // Every file copied that is as it was then goes; anything else (added
    // or changed since) is merged in, never over a different file.
    for (rel, size, modified) in &copied {
        let p = from.join(rel);
        let same = std::fs::symlink_metadata(&p)
            .is_ok_and(|m| m.is_file() && m.len() == *size && m.modified().ok() == Some(*modified));
        if same {
            std::fs::remove_file(&p)?;
        }
    }
    merge_tree(from, to)?;
    Ok(copied.len())
}

/// (path under the folder, size, modified) of each file copied.
type Copied = Vec<(PathBuf, u64, SystemTime)>;

/// `from` copied into the new folder `to`, every file checked.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<Copied> {
    let mut copied = Vec::new();
    std::fs::create_dir(to)?;
    for e in walkdir::WalkDir::new(from).follow_links(false).min_depth(1) {
        let e = e.map_err(std::io::Error::from)?;
        let rel = e.path().strip_prefix(from).unwrap_or(e.path()).to_owned();
        let ft = e.file_type();
        if ft.is_dir() {
            std::fs::create_dir(to.join(&rel))?;
        } else if ft.is_file() {
            let meta = e.metadata().map_err(std::io::Error::from)?;
            copy_checked(e.path(), &to.join(&rel))?;
            copied.push((rel, meta.len(), meta.modified()?));
        } else {
            return Err(std::io::Error::other(format!(
                "{} is a link or special file; it isn't moved",
                e.path().display()
            )));
        }
    }
    Ok(copied)
}

/// Move what is under `from` into `to`, which may already hold some of it:
/// a file already there as the same file just goes from `from`; one that
/// isn't there moves (never replacing anything); one that differs stays
/// and is an error. Folders follow, empty ones too, and `from` goes once
/// empty. Safe to run again after a failure. Returns the files moved or
/// found there already.
pub(crate) fn merge_tree(from: &Path, to: &Path) -> Result<usize, Error> {
    let mut n = 0;
    let mut differ = Vec::new();
    std::fs::create_dir_all(to)?;
    for e in walkdir::WalkDir::new(from).follow_links(false).min_depth(1) {
        let e = e.map_err(std::io::Error::from)?;
        let rel = e.path().strip_prefix(from).unwrap_or(e.path());
        let dst = to.join(rel);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&dst)?;
            continue;
        }
        if dst.symlink_metadata().is_ok() {
            if e.file_type().is_file() && same_file(e.path(), &dst)? {
                std::fs::remove_file(e.path())?;
                n += 1;
            } else {
                differ.push(rel.display().to_string());
            }
            continue;
        }
        move_file(e.path(), &dst)?;
        n += 1;
    }
    // Folders left empty go (contents first); `from` with them.
    for e in walkdir::WalkDir::new(from).contents_first(true) {
        if let Ok(e) = e
            && e.file_type().is_dir()
        {
            let _ = std::fs::remove_dir(e.path());
        }
    }
    if !differ.is_empty() {
        return Err(Error::Io(std::io::Error::other(format!(
            "{} holds files that differ from those in {}, so they stay: {}",
            from.display(),
            to.display(),
            differ.join(", ")
        ))));
    }
    if from.symlink_metadata().is_ok() {
        return Err(Error::Io(std::io::Error::other(format!(
            "{} couldn't be removed after its files moved to {}",
            from.display(),
            to.display()
        ))));
    }
    Ok(n)
}
