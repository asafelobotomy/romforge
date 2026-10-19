//! The list view's cache (migration 35's generation, and the snapshot
//! beside the database) never shows anything but what a fresh build
//! would: checked after each kind of change. On copies in temp folders.

use std::path::{Path, PathBuf};

use romforge_core::settings::Settings;
use romforge_core::{Library, Scope};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn same(lib: &Library, after: &str) {
    assert_eq!(
        lib.library_view().unwrap(),
        lib.library_view_fresh().unwrap(),
        "after {after}"
    );
}

#[test]
fn the_cached_view_is_always_a_fresh_one() {
    let tmp = tempfile::tempdir().unwrap();
    let roms = tmp.path().join("roms");
    std::fs::create_dir_all(&roms).unwrap();
    for f in ["gb/Test GB.gb", "nes/Test NES (iNES).nes", "unknown.bin"] {
        let from = fixtures().join("roms").join(f);
        std::fs::copy(&from, roms.join(from.file_name().unwrap())).unwrap();
    }
    let db = tmp.path().join("library.db");
    let mut lib = Library::open(&db).unwrap();
    same(&lib, "opening");
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    same(&lib, "a DAT import");
    lib.scan(&roms, Scope::All).unwrap();
    same(&lib, "a scan");
    let g = lib.view_generation().unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    assert_eq!(
        lib.view_generation().unwrap(),
        g,
        "a rescan that finds nothing new changes nothing"
    );
    same(&lib, "a rescan");
    lib.import_dat(fixtures().join("dats/nes.dat"), None).unwrap();
    same(&lib, "a second DAT (the NES ROM now matches)");
    let unknown = lib
        .library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.title.ends_with("unknown.bin"))
        .unwrap()
        .id;
    lib.identify_as(&unknown, "Game Boy", "My Homebrew").unwrap();
    same(&lib, "naming a file");
    let mut s = lib.settings().unwrap();
    s.regions = vec!["Japan".into(), "USA".into()];
    lib.save_settings(&s).unwrap();
    same(&lib, "a new region order");
    let named = lib
        .library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.title == "My Homebrew")
        .unwrap()
        .id;
    lib.forget_identity(&named).unwrap();
    same(&lib, "forgetting a name");
    std::fs::remove_file(roms.join("Test GB.gb")).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    same(&lib, "a file going away");
    // Unrelated settings leave the list as it is.
    let g = lib.view_generation().unwrap();
    let mut s = lib.settings().unwrap();
    s.show_empty_media = true;
    lib.save_settings(&s).unwrap();
    assert_eq!(
        lib.view_generation().unwrap(),
        g,
        "Details' setting isn't the list's"
    );

    // Opened again, the snapshot beside the database serves the view.
    let before = lib.library_view().unwrap();
    drop(lib);
    let snapshot = PathBuf::from(format!("{}-view.json", db.canonicalize().unwrap().display()));
    assert!(snapshot.exists(), "{}", snapshot.display());
    let lib = Library::open(&db).unwrap();
    assert_eq!(lib.library_view().unwrap(), before);
    same(&lib, "reopening");
    // A snapshot that isn't this generation's is never used.
    let mut lib = lib;
    lib.save_settings(&Settings {
        regions: vec!["USA".into()],
        ..lib.settings().unwrap()
    })
    .unwrap();
    same(&lib, "a change after reopening");
    let (g, view) = lib
        .library_view_since(Some(lib.view_generation().unwrap()))
        .unwrap();
    assert!(view.is_none(), "the app's view is current: nothing sent");
    assert_eq!(g, lib.view_generation().unwrap());
    assert!(lib.library_view_since(Some(g - 1)).unwrap().1.is_some());
}
