//! An import cut short (the app closed, the process killed) after a game
//! was written but before the batch ended: the next import finds it
//! already there and takes the original, instead of skipping it with
//! "already exists"; the staging folder and a half-written game go.
//! Always on copies in temp folders.

use romforge_core::home::Home;
use romforge_core::{Library, Scope};

mod common;
use common::fixtures;
use common::home::*;

#[test]
fn the_next_import_finishes_what_one_cut_short_left() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let rom = fixtures().join("roms/gb/Test GB.gb");
    std::fs::copy(&rom, incoming.join("Test GB.gb")).unwrap();
    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let plan = lib.import_plan(Some(&incoming)).unwrap();
    let target = plan.items[0].target.clone();
    lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(target.is_file());

    // As if it stopped right after writing: the original never went to
    // the trash, the library never scanned Games/, a staging folder and
    // another game's half-written file are left.
    std::fs::copy(&rom, incoming.join("Test GB.gb")).unwrap();
    let conn = rusqlite::Connection::open(home.db()).unwrap();
    conn.execute("DELETE FROM files WHERE path = ?1", [target.to_string_lossy()])
        .unwrap();
    let cut = target.with_file_name("Cut.chd");
    let part = target.with_file_name(".Cut.chd.romforge-part");
    std::fs::write(&part, b"half").unwrap();
    conn.execute(
        "INSERT INTO cleanup_outputs (batch_id, path, kind)
         SELECT max(batch_id), ?1, 'file' FROM cleanup_batches",
        [cut.to_string_lossy()],
    )
    .unwrap();
    let stale = home.staging().join("batch-9");
    std::fs::create_dir_all(stale.join("0")).unwrap();
    drop(conn);
    lib.scan(&incoming, Scope::All).unwrap();
    let before = lib.import_plan(Some(&incoming)).unwrap();
    assert!(before.items.is_empty(), "the bug: {before:?}");

    let r = lib.recover_imports().unwrap();
    assert_eq!((r.staging, r.parts, r.scanned, r.busy), (1, 1, 1, false), "{r:?}");
    assert!(!stale.exists() && !part.exists());
    let plan = lib.import_plan(Some(&incoming)).unwrap();
    assert!(plan.items.len() == 1 && plan.items[0].existing, "{plan:?}");
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert_eq!(r.trashed, 1, "{r:?}");
    assert!(
        files_under(&incoming).is_empty(),
        "the original went to the trash"
    );
    assert!(target.is_file(), "the game stays in Games/");

    // Nothing left to tidy.
    let r = lib.recover_imports().unwrap();
    assert_eq!((r.staging, r.parts, r.scanned), (0, 0, 0), "{r:?}");
}
