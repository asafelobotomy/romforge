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
