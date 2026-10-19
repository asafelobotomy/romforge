//! Milestone 13: one RomForge folder. Always on copies in temp folders.

use std::path::Path;

use romforge_core::home::Home;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

mod common;
use common::fixtures;
use common::home::*;

/// Add a folder, see the plan, import: identified games move into
/// `Games/`, the originals into the RomForge trash, the rest stays; undo
/// puts everything back.
#[test]
fn import_moves_games_into_romforge_and_undo_restores() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(&tmp.path().join("parent")).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    for f in ["Test GB.gb", "CRC Only.gb"] {
        std::fs::copy(fixtures().join("roms/gb").join(f), incoming.join(f)).unwrap();
    }
    std::fs::write(incoming.join("mystery.gb"), vec![1u8; 512]).unwrap();
    let before = files_under(&incoming);

    let mut lib = Library::open(home.db()).unwrap();
    assert_eq!(lib.home(), Some(&home));
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    assert_eq!(files_under(&incoming), before, "scanning moves nothing");

    let plan = lib.import_plan(Some(&incoming)).unwrap();
    assert_eq!(plan.dest, home.games());
    let targets: Vec<&Path> = plan.items.iter().map(|i| i.target.as_path()).collect();
    assert_eq!(
        targets,
        [home.games().join("Game Boy/Test GB.zip").as_path()],
        "{plan:?}"
    );
    assert_eq!(files_under(&incoming), before, "planning moves nothing");

    let report = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(report.trashed, 1);
    assert_eq!(
        files_under(&incoming),
        ["CRC Only.gb", "mystery.gb"],
        "only the unidentified stay"
    );
    assert!(home.games().join("Game Boy/Test GB.zip").is_file());
    let trashed = files_under(&home.trash());
    assert_eq!(trashed.len(), 1);
    assert!(trashed[0].ends_with("incoming/Test GB.gb"), "{trashed:?}");
    let files = lib.files(&romforge_core::LibraryFilter::default()).unwrap();
    assert!(
        files.iter().all(|f| !f.display_path().contains("romforge/")),
        "an archive's own romforge/ folder is never scanned"
    );
    let rows = lib.library_view().unwrap().rows;
    let gb = rows.iter().find(|r| r.title == "Test GB").unwrap();
    assert_eq!(gb.status, RowStatus::Verified, "found again in Games by refresh");

    // Nothing left to import from there; the RomForge folder itself never is.
    assert!(lib.import_plan(Some(&incoming)).unwrap().items.is_empty());
    assert!(lib.import_plan(Some(&home.games())).is_err());

    // Another copy of a game already in Games/ goes to the trash as a copy.
    let again = tmp.path().join("again");
    std::fs::create_dir_all(&again).unwrap();
    std::fs::copy(
        fixtures().join("roms/gb/Test GB.gb"),
        again.join("Test GB (copy).gb"),
    )
    .unwrap();
    lib.scan(&again, Scope::All).unwrap();
    let plan = lib.import_plan(Some(&again)).unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    assert!(plan.items[0].existing && plan.items[0].parts.is_empty());
    assert_eq!(plan.items[0].target, home.games().join("Game Boy/Test GB.zip"));
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert_eq!((r.done, r.trashed), (0, 1), "{r:?}");
    assert!(files_under(&again).is_empty());
    lib.cleanup_undo(None).unwrap();
    assert_eq!(
        files_under(&again),
        ["Test GB (copy).gb"],
        "undo brings the copy back"
    );

    lib.cleanup_undo(None).unwrap();
    assert_eq!(files_under(&incoming), before);
    assert!(files_under(&home.games()).is_empty());
}

/// Art fetched for a library in a RomForge folder goes to its `Media/`,
/// and downloaded databases to its `Databases/`.
#[test]
fn art_and_databases_live_in_romforge() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let lib = Library::open(home.db()).unwrap();
    assert_eq!(lib.extdb_dir(), Some(home.databases()));
    assert!(
        lib.organize_plan(&romforge_core::organize::OrganizeOptions::new(
            tmp.path(),
            home.games()
        ))
        .is_err(),
        "Import does Organize's job here"
    );
    // Keep originals: imports go into RomForge, no Library folder needed.
    let mut lib = lib;
    let keep = romforge_core::settings::Settings {
        keep_originals: true,
        ..Default::default()
    };
    assert!(lib.save_settings(&keep).unwrap().keep_originals);
    let mut elsewhere = Library::open(tmp.path().join("plain.db")).unwrap();
    assert!(
        elsewhere.save_settings(&keep).is_err(),
        "without RomForge it needs a Library"
    );
    assert_eq!(elsewhere.home(), None);
    assert_eq!(elsewhere.extdb_dir(), Some(tmp.path().join("databases")));
}

/// Emptying the trash deletes the originals kept for undo, for good: the
/// batch can't be undone after, and its new files stay.
#[test]
fn emptying_the_trash_ends_undo() {
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
    assert_eq!(r.trashed, 1);

    assert!(
        lib.trash_contents(Some(1)).unwrap().batches.is_empty(),
        "nothing a day old yet"
    );
    let t = lib.trash_contents(None).unwrap();
    assert_eq!((t.batches.len(), t.files), (1, 1), "{t:?}");
    assert!(t.bytes > 0);
    assert_eq!(files_under(&home.trash()).len(), 1, "listing deletes nothing");

    let gone = lib.empty_trash(None).unwrap();
    assert!(gone.failed.is_empty(), "{:?}", gone.failed);
    assert_eq!(gone.files, 1);
    assert!(files_under(&home.trash()).is_empty());
    assert!(home.trash().is_dir(), "the trash folder itself stays");
    assert!(lib.trash_contents(None).unwrap().batches.is_empty());

    let err = lib.cleanup_undo(None).unwrap_err().to_string();
    assert!(
        err.contains("nothing to undo"),
        "the latest undoable batch is none: {err}"
    );
    let err = lib.cleanup_undo(Some(r.batch_id)).unwrap_err().to_string();
    assert!(err.contains("can't be undone"), "{err}");
    assert!(
        home.games().join("Game Boy/Test GB.zip").is_file(),
        "the new file stays"
    );
    let b = lib.cleanup_batches().unwrap();
    assert!(b[0].emptied_at.is_some());
}

/// Files skipped for their extension are listed by kind (details files
/// left out), so a format romforge doesn't know shows. A loose folder named
/// `romforge` could be the owner's, so what is in it is listed.
#[test]
fn skipped_files_are_listed_by_kind() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("mixed");
    std::fs::create_dir_all(dir.join("romforge")).unwrap();
    for f in [
        "a.txt",
        "b.TXT",
        "c.nfo",
        "README",
        "Game.romforge.md",
        "romforge/boxart.png",
    ] {
        std::fs::write(dir.join(f), b"x").unwrap();
    }
    let mut lib = Library::open(tmp.path().join("lib.db")).unwrap();
    let r = lib.scan(&dir, Scope::All).unwrap();
    assert_eq!(r.skipped, 6);
    let kinds: Vec<(&str, usize)> = r
        .skipped_kinds
        .iter()
        .map(|k| (k.ext.as_str(), k.count))
        .collect();
    assert_eq!(
        kinds,
        [("txt", 2), ("", 1), ("nfo", 1), ("png", 1)],
        "{:?}",
        r.skipped_kinds
    );
    assert_eq!(
        romforge_core::SkippedKind::summary(&r.skipped_kinds, 2),
        ".txt 2, no extension 1, and 2 more kinds"
    );
}
