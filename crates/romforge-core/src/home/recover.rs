//! Where the library is after making or moving the RomForge folder, for a
//! shell that closed its connections first and opens them again.

use std::path::{Path, PathBuf};

use super::{Created, Home};
use crate::Error;

/// After [`super::create_or_adopt`] into `parent`, with the library at
/// `old_db` before: in the folder when it worked; when it failed, still
/// where it was, else in the folder if it moved before the failure; `None`
/// when it is found in neither place.
pub fn db_after_create(parent: &Path, old_db: &Path, done: &Result<Created, Error>) -> Option<PathBuf> {
    match done {
        Ok(d) => Some(d.home().db()),
        Err(_) if old_db.is_file() => Some(old_db.to_owned()),
        Err(_) => Home::in_parent(parent).map(|h| h.db()).filter(|p| p.is_file()),
    }
}

/// After [`super::relocate`] of `old`: in the new place when it moved,
/// else where it was.
pub fn db_after_move(old: &Home, moved: &Result<Home, Error>) -> PathBuf {
    moved.as_ref().map_or_else(|_| old.db(), Home::db)
}

/// What making or moving the RomForge folder did, and where the library's
/// database is now, worked or not (`None`: found in neither place).
#[derive(Debug)]
pub struct Made<T> {
    pub result: Result<T, Error>,
    pub db: Option<PathBuf>,
}

/// Make (or adopt) the RomForge folder in `parent`, moving the library at
/// `old_db` (and the loose art beside the cache) into it, and use it from
/// now on. For a shell to call with its connections closed: [`Made::db`]
/// says where to open them again.
pub fn create(parent: &Path, current: Option<&Home>, old_db: &Path) -> Made<Created> {
    let art = crate::enrich::default_cache_dir().join(crate::layout::LOOSE_ART_DIR);
    let done = super::create_or_adopt(parent, current, old_db, &art);
    let db = db_after_create(parent, old_db, &done);
    Made {
        result: done.and_then(|c| c.home().remember().map(|()| c)),
        db,
    }
}

/// Move the RomForge folder `old` into `new_parent` and use it from now on;
/// as [`create`], for a shell with its connections closed.
pub fn move_to(old: &Home, new_parent: &Path) -> Made<Home> {
    let moved = super::relocate(old, new_parent);
    let db = Some(db_after_move(old, &moved));
    Made {
        result: moved.and_then(|h| h.remember().map(|()| h)),
        db,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_create_keeps_the_old_library() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("library.db");
        std::fs::write(&old, b"").unwrap();
        let failed: Result<Created, Error> = Err(Error::Settings("no".into()));
        assert_eq!(db_after_create(dir.path(), &old, &failed), Some(old.clone()));
        std::fs::remove_file(&old).unwrap();
        assert_eq!(db_after_create(dir.path(), &old, &failed), None, "found nowhere");
    }
}
