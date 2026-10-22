//! A game already in Games/: its copies outside go to the trash only when
//! Games/ still holds it as planned. Always on copies in temp folders.

use std::io::Write;

use romforge_core::home::Home;
use romforge_core::{Library, Scope};

mod common;
use common::fixtures;
use common::home::*;

#[test]
fn copies_stay_when_the_game_in_games_changed_since_the_plan() {
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
    lib.cleanup_apply(&plan, |_, _| {}).unwrap();

    let again = tmp.path().join("again");
    std::fs::create_dir_all(&again).unwrap();
    std::fs::copy(&rom, again.join("Test GB (copy).gb")).unwrap();
    lib.scan(&again, Scope::All).unwrap();
    let plan = lib.import_plan(Some(&again)).unwrap();
    assert!(plan.items.len() == 1 && plan.items[0].existing, "{plan:?}");

    // The zip in Games/ now holds something else under the same name.
    let target = &plan.items[0].target;
    std::fs::remove_file(target).unwrap();
    let mut z = zip::ZipWriter::new(std::fs::File::create(target).unwrap());
    z.start_file("Test GB.gb", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(b"not the game").unwrap();
    z.finish().unwrap();

    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert_eq!(r.trashed, 0, "{r:?}");
    assert!(r.failed[0].1.contains("no longer holds"), "{:?}", r.failed);
    assert_eq!(files_under(&again), ["Test GB (copy).gb"], "the copy stays");

    // And gone altogether.
    std::fs::remove_file(target).unwrap();
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert_eq!(r.trashed, 0, "{r:?}");
    assert!(r.failed[0].1.contains("is gone"), "{:?}", r.failed);
    assert_eq!(files_under(&again), ["Test GB (copy).gb"]);
}
