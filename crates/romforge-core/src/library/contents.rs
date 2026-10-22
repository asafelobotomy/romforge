//! What the library holds, asked cheaply: whether it has anything, and
//! the files under one folder.

use std::path::Path;

use crate::{Error, Library, LibraryFile, LibraryFilter};

impl Library {
    /// Nothing scanned, no folder games or patches, no DAT: what the list
    /// would show is empty. A few `EXISTS` instead of building the list.
    pub fn is_empty(&self) -> Result<bool, Error> {
        let any: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM files) OR EXISTS(SELECT 1 FROM folder_games)
                 OR EXISTS(SELECT 1 FROM patch_files) OR EXISTS(SELECT 1 FROM dats)",
            [],
            |r| r.get(0),
        )?;
        Ok(!any)
    }

    /// The library's files inside folder `root` (whole path components:
    /// `/roms/snes` holds `/roms/snes/a.sfc`, not `/roms/snes-hacks/b.sfc`).
    pub fn files_under(&self, root: &Path) -> Result<Vec<LibraryFile>, Error> {
        let mut files = self.files(&LibraryFilter::default())?;
        files.retain(|f| Path::new(&f.path).starts_with(root));
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_then_not_and_files_under_a_folder_only() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::open_in_memory().unwrap();
        assert!(lib.is_empty().unwrap());
        for sub in ["snes", "snes-hacks"] {
            let d = dir.path().join(sub);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(format!("{sub}.sfc")), b"not a real rom").unwrap();
        }
        let opts = crate::ScanOptions {
            all_files: true,
            ..Default::default()
        };
        lib.scan_with(dir.path(), crate::Scope::All, &opts, |_| {})
            .unwrap();
        assert!(!lib.is_empty().unwrap());
        let under = lib.files_under(&dir.path().join("snes")).unwrap();
        assert_eq!(under.len(), 1, "{under:?}");
        assert!(under[0].path.ends_with("snes.sfc"));
    }
}
