//! Milestone 10: cleanup & rename, on copies in temp folders. Every new
//! file is read back before an original is touched; undo puts everything
//! back.

use std::io::Read as _;

use romforge_core::cleanup::Format;
use romforge_core::layout::TRASH_DIR;
use romforge_core::settings::Settings;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

mod common;
use common::cleanup::*;

#[test]
fn cartridges_are_zipped_under_their_dat_names_and_undo_restores() {
    let mut s = setup(
        false,
        &[
            ("gb/Test GB.gb", "loose/tgb.gb"),
            ("archives/Test GBA.zip", "Test GBA.zip"),
            ("archives/Mixed.7z", "Mixed.7z"),
        ],
        &["gb.dat", "gba.dat", "md.dat", "nes.dat"],
    );
    let plan = s.lib.cleanup_plan(None).unwrap();
    let titles: Vec<&str> = plan.items.iter().map(|i| i.title.as_str()).collect();
    assert!(
        titles.contains(&"Test GB") && titles.contains(&"Test MD"),
        "{titles:?} {:?}",
        plan.skipped
    );
    assert!(plan.items.iter().all(|i| i.format == Format::Zip));
    // Mixed.7z holds notes.txt too, so it stays; Test GBA.zip has a .url.
    let md = plan.items.iter().find(|i| i.title == "Test MD").unwrap();
    assert!(
        md.trash.is_empty() && md.kept == [s.source.join("Mixed.7z")],
        "{md:?}"
    );
    let gb = plan.items.iter().find(|i| i.title == "Test GB").unwrap();
    assert_eq!(gb.target, s.library.join("Game Boy/Test GB.zip"));
    assert_eq!(gb.trash, [s.source.join("loose/tgb.gb")]);
    let original = std::fs::read(s.source.join("loose/tgb.gb")).unwrap();

    let report = s.lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(report.done, plan.items.len());
    assert_eq!(report.trashed, 1);

    // The new zip: the ROM under its DAT name and the details file.
    let zip = s.library.join("Game Boy/Test GB.zip");
    assert_eq!(zip_names(&zip), ["Test GB.gb", "romforge/Test GB.romforge.md"]);
    let mut z = zip::ZipArchive::new(std::fs::File::open(&zip).unwrap()).unwrap();
    let mut md_text = String::new();
    z.by_name("romforge/Test GB.romforge.md")
        .unwrap()
        .read_to_string(&mut md_text)
        .unwrap();
    assert!(
        md_text.starts_with("# Test GB\n") && md_text.contains("verified by hash"),
        "{md_text}"
    );
    let mut rom = Vec::new();
    z.by_name("Test GB.gb").unwrap().read_to_end(&mut rom).unwrap();
    assert_eq!(rom, original);
    // A 7z member came out whole too.
    let md_zip = s.library.join("Genesis/Test MD.zip");
    assert!(zip_names(&md_zip).contains(&"Test MD.md".to_owned()));

    // The original is in the trash, not gone; the library now shows the
    // new copy, verified, and nothing from the trash.
    assert!(!s.source.join("loose/tgb.gb").exists());
    let trashed = s
        .source
        .join("loose")
        .join(TRASH_DIR)
        .join(report.batch_id.to_string())
        .join("tgb.gb");
    assert_eq!(std::fs::read(&trashed).unwrap(), original);
    assert!(s.source.join("Mixed.7z").exists());
    let rows = row_status(&s.lib, "Test GB");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].0, RowStatus::Verified);
    assert!(rows[0].1.starts_with(&*zip.to_string_lossy()), "{rows:?}");
    // Nothing left to do for it.
    let again = s.lib.cleanup_plan(None).unwrap();
    assert!(again.items.iter().all(|i| i.title != "Test GB"), "{again:?}");
    assert_eq!(s.lib.cleanup_batches().unwrap().len(), 1);

    let undone = s.lib.cleanup_undo(None).unwrap();
    assert!(undone.failed.is_empty(), "{:?}", undone.failed);
    assert_eq!(undone.restored, 1);
    assert_eq!(std::fs::read(s.source.join("loose/tgb.gb")).unwrap(), original);
    assert!(!zip.exists() && !md_zip.exists());
    assert!(
        !s.source.join("loose").join(TRASH_DIR).exists(),
        "empty trash folders go"
    );
    let rows = row_status(&s.lib, "Test GB");
    assert!(rows[0].1.ends_with("loose/tgb.gb"), "{rows:?}");
    assert!(s.lib.cleanup_batches().unwrap()[0].undone_at.is_some());
}

#[test]
fn keep_originals_leaves_source_alone_and_nothing_is_overwritten() {
    let mut s = setup(
        true,
        &[("gb/Test GB.gb", "Test GB.gb"), ("gb/MD5 Only.gb", "MD5 Only.gb")],
        &["gb.dat"],
    );
    // Something already has MD5 Only's name in the Library.
    std::fs::create_dir_all(s.library.join("Game Boy")).unwrap();
    std::fs::write(s.library.join("Game Boy/MD5 Only.zip"), b"mine").unwrap();
    let plan = s.lib.cleanup_plan(None).unwrap();
    assert!(plan.keep_originals);
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    assert!(
        plan.skipped
            .iter()
            .any(|(t, why)| t == "MD5 Only" && why.contains("already exists"))
    );
    let report = s.lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert_eq!((report.done, report.trashed), (1, 0));
    assert!(s.source.join("Test GB.gb").exists());
    assert!(!s.source.join(TRASH_DIR).exists());
    assert_eq!(
        std::fs::read(s.library.join("Game Boy/MD5 Only.zip")).unwrap(),
        b"mine"
    );
    assert!(s.library.join("Game Boy/Test GB.zip").exists());
    // Staging is cleaned up after the batch.
    let staging = s.source.parent().unwrap().join("staging");
    assert!(!staging.exists() || std::fs::read_dir(&staging).unwrap().next().is_none());
}

#[test]
fn cleanup_needs_a_source_folder() {
    let lib = Library::open_in_memory().unwrap();
    assert!(lib.cleanup_plan(None).is_err());
}

/// A DAT's ROM name is untrusted: one that would climb out of the archive
/// skips the game.
#[test]
fn unsafe_dat_names_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (source, library) = (dir.path().join("source"), dir.path().join("library"));
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    let data = pattern(5, 1024);
    std::fs::write(source.join("x.gb"), &data).unwrap();
    let dat = disc_dat(
        dir.path(),
        "Nintendo - Game Boy",
        &[("Evil (USA)", vec![("../../evil.gb".into(), data)])],
    );
    let lib = lib_with(dir.path(), &dat, &source, &library);
    let plan = lib.cleanup_plan(None).unwrap();
    assert!(plan.items.is_empty(), "{plan:?}");
    assert!(
        plan.skipped.iter().any(|(_, why)| why.contains("safely")),
        "{:?}",
        plan.skipped
    );
}

/// Undo keeps the new files while an original can't come back (its place
/// is taken), so the trash is never the only copy left.
#[test]
fn undo_keeps_new_files_until_every_original_is_back() {
    let s = setup(false, &[("gb/Test GB.gb", "Test GB.gb")], &["gb.dat"]);
    let mut lib = s.lib;
    let plan = lib.cleanup_plan(None).unwrap();
    let item = plan.items.iter().find(|i| i.title == "Test GB").unwrap().clone();
    let report = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(item.target.exists());
    // Something else now sits where the original was.
    std::fs::write(s.source.join("Test GB.gb"), b"someone else's file").unwrap();
    let undone = lib.cleanup_undo(None).unwrap();
    assert!(!undone.failed.is_empty());
    assert!(
        item.target.exists(),
        "the new zip stays while the original is in the trash"
    );
    // Once the place is free, undo finishes the job.
    std::fs::remove_file(s.source.join("Test GB.gb")).unwrap();
    let again = lib.cleanup_undo(None).unwrap();
    assert!(again.failed.is_empty(), "{:?}", again.failed);
    assert!(!item.target.exists());
    assert!(s.source.join("Test GB.gb").exists());
}

/// A game you named that sits in Source twice: one copy goes in the
/// archive, the other is a copy, not a second ROM.
#[test]
fn a_named_game_twice_is_one_part() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let library = dir.path().join("library");
    std::fs::create_dir_all(source.join("b")).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    let data = vec![3u8; 2048];
    std::fs::write(source.join("mine.gb"), &data).unwrap();
    std::fs::write(source.join("b/mine copy.gb"), &data).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.save_settings(&Settings {
        source_dir: Some(source.clone()),
        library_dir: Some(library.clone()),
        staging_dir: Some(dir.path().join("staging")),
        ..Settings::default()
    })
    .unwrap();
    lib.scan(&source, Scope::All).unwrap();
    let row = lib.search_queue(None).unwrap()[0].row_id.clone();
    lib.identify_as(&row, "Game Boy", "My Game").unwrap();
    let plan = lib.cleanup_plan(None).unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    assert_eq!(plan.items[0].parts.len(), 1, "{:?}", plan.items[0].parts);
    assert_eq!(plan.items[0].copies.len(), 1);
}
