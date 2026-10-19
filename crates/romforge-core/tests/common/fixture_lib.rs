//! Helpers from the `fixtures` tests.

use std::collections::BTreeMap;

use romforge_core::{Library, LibraryFilter};

use super::fixtures;

pub fn import_all(lib: &mut Library) {
    let mut dats: Vec<_> = std::fs::read_dir(fixtures().join("dats"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    dats.sort();
    assert_eq!(dats.len(), 8);
    for d in dats {
        lib.import_dat(&d, None).unwrap();
    }
}

/// fixture path (relative to fixtures/roms, `#member` for archives) -> file
pub fn by_path(lib: &Library) -> BTreeMap<String, romforge_core::LibraryFile> {
    let roms = std::fs::canonicalize(fixtures().join("roms")).unwrap();
    let prefix = format!("{}/", roms.display());
    lib.files(&LibraryFilter::default())
        .unwrap()
        .into_iter()
        .map(|f| (f.display_path().strip_prefix(&prefix).unwrap().to_owned(), f))
        .collect()
}

pub fn dat_id(lib: &Library, name: &str) -> i64 {
    let s = lib.summary().unwrap();
    s.dats.iter().find(|d| d.name.starts_with(name)).unwrap().dat_id
}
