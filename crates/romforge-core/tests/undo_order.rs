//! Cleanup undo goes newest first, and sets what it takes away aside in
//! the trash rather than deleting it. Always on copies in temp folders.

use romforge_core::home::Home;
use romforge_core::settings::Settings;
use romforge_core::{Library, Scope};

mod common;
use common::fixtures;
use common::home::*;

/// One import with "keep originals" (nothing trashed), a second one that
/// trashes the outside copy of the game now in `Games/`, then the trash
/// emptied: the first import's zip is the only copy left, so undo must not
/// take it away.
#[test]
fn undo_never_skips_past_an_emptied_batch() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    std::fs::copy(fixtures().join("roms/gb/Test GB.gb"), incoming.join("Test GB.gb")).unwrap();
    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    let keep = |on: bool| Settings {
        keep_originals: on,
        ..Default::default()
    };
    lib.save_settings(&keep(true)).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let plan = lib.import_plan(Some(&incoming)).unwrap();
    let b1 = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert_eq!((b1.done, b1.trashed), (1, 0), "{b1:?}");
    let zip = home.games().join("Game Boy/Test GB.zip");
    assert!(zip.is_file());

    lib.save_settings(&keep(false)).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let plan = lib.import_plan(Some(&incoming)).unwrap();
    assert!(plan.items.iter().all(|i| i.existing), "{plan:?}");
    let b2 = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert_eq!(b2.trashed, 1, "{b2:?}");
    assert!(files_under(&incoming).is_empty());
    lib.empty_trash(None).unwrap();

    let err = lib.cleanup_undo(None).unwrap_err().to_string();
    assert!(err.contains("can't be undone"), "{err}");
    let err = lib.cleanup_undo(Some(b1.batch_id)).unwrap_err().to_string();
    assert!(err.contains("newest first"), "{err}");
    assert!(zip.is_file(), "the only copy left stays");
}

/// What undo takes away goes to the trash (`undo-<batch>`), where emptying
/// the trash finds it.
#[test]
fn undo_sets_new_files_aside_in_the_trash() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    std::fs::copy(fixtures().join("roms/gb/Test GB.gb"), incoming.join("Test GB.gb")).unwrap();
    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let plan = lib.import_plan(Some(&incoming)).unwrap();
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    let zip = home.games().join("Game Boy/Test GB.zip");
    let written = std::fs::read(&zip).unwrap();
    let u = lib.cleanup_undo(None).unwrap();
    assert!(u.failed.is_empty(), "{u:?}");
    assert_eq!(files_under(&incoming), ["Test GB.gb"]);
    assert!(!zip.exists());
    let aside = home.trash().join(format!("undo-{}", r.batch_id));
    let kept = files_under(&aside);
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(std::fs::read(aside.join(&kept[0])).unwrap(), written);

    let t = lib.trash_contents(None).unwrap();
    assert_eq!(t.files, 1, "{t:?}");
    let gone = lib.empty_trash(None).unwrap();
    assert!(gone.failed.is_empty(), "{:?}", gone.failed);
    assert!(files_under(&home.trash()).is_empty());
}
