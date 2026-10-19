//! Milestone 10: cleanup & rename, on copies in temp folders. Every new
//! file is read back before an original is touched; undo puts everything
//! back.
//!
//! Replacing originals in place and converting where the discs are.

use romforge_core::cleanup::Format;
use romforge_core::settings::Settings;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

mod common;
use common::cleanup::*;
use common::fixtures;

/// "Replace originals": the game is renamed and repacked in its own folder,
/// not moved into the Library; the original goes to the trash.
#[test]
fn replace_originals_works_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let library = dir.path().join("library");
    std::fs::create_dir_all(source.join("handheld")).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    std::fs::copy(
        fixtures().join("roms/gb/Test GB.gb"),
        source.join("handheld/tgb.gb"),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.save_settings(&Settings {
        source_dir: Some(source.clone()),
        library_dir: Some(library.clone()),
        in_place: true,
        staging_dir: Some(dir.path().join("staging")),
        ..Settings::default()
    })
    .unwrap();
    lib.scan(&source, Scope::All).unwrap();
    let source = source.canonicalize().unwrap();
    let plan = lib.cleanup_plan(None).unwrap();
    assert!(plan.in_place);
    let gb = plan.items.iter().find(|i| i.title == "Test GB").expect("planned");
    assert_eq!(
        gb.target,
        source.join("handheld/Test GB.zip"),
        "beside the original"
    );
    let report = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(source.join("handheld/Test GB.zip").exists());
    assert!(
        !source.join("handheld/tgb.gb").exists(),
        "the original went to the trash"
    );
    assert!(
        std::fs::read_dir(&library).unwrap().next().is_none(),
        "nothing in the Library"
    );
    lib.cleanup_undo(None).unwrap();
    assert!(source.join("handheld/tgb.gb").exists(), "undo puts it back");
    assert!(!source.join("handheld/Test GB.zip").exists());
    // It can't both replace and keep the originals.
    assert!(
        lib.save_settings(&Settings {
            source_dir: Some(source.clone()),
            library_dir: Some(library),
            in_place: true,
            keep_originals: true,
            ..Settings::default()
        })
        .is_err()
    );
}

/// Convert: a disc becomes a CHD beside itself under its own name (not
/// renamed or moved), the originals to the trash; a GameCube image, which
/// its emulator can't read as a CHD, is left alone and says why.
#[test]
fn convert_makes_chds_where_the_discs_are() {
    let dir = tempfile::tempdir().unwrap();
    let (source, library) = (dir.path().join("source"), dir.path().join("library"));
    std::fs::create_dir_all(&library).unwrap();
    let game = "Game (USA)";
    let d = source.join("my discs");
    std::fs::create_dir_all(&d).unwrap();
    let (t1, t2) = (cd_tracks(1), pattern(5, 155 * 2352));
    std::fs::write(d.join(format!("{game} (Track 1).bin")), &t1).unwrap();
    std::fs::write(d.join(format!("{game} (Track 2).bin")), &t2).unwrap();
    std::fs::write(d.join(format!("{game}.cue")), cue_for(game)).unwrap();
    let ps1 = disc_dat(
        dir.path(),
        "Sony - PlayStation",
        &[(
            game,
            vec![
                (format!("{game}.cue"), cue_for(game).into_bytes()),
                (format!("{game} (Track 1).bin"), t1),
                (format!("{game} (Track 2).bin"), t2),
            ],
        )],
    );
    let iso = pattern(7, 4096);
    std::fs::write(d.join("cube.iso"), &iso).unwrap();
    let gc = disc_dat(
        dir.path(),
        "Nintendo - GameCube",
        &[("Cube (USA)", vec![("Cube (USA).iso".into(), iso)])],
    );
    let mut lib = lib_with(dir.path(), &ps1, &source, &library);
    lib.import_dat(&gc, Some("GameCube")).unwrap();
    lib.scan(&source, Scope::All).unwrap();
    let d = d.canonicalize().unwrap();

    let plan = lib.convert_plan(None).unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    assert_eq!(plan.items[0].target, d.join(format!("{game}.chd")));
    assert_eq!(plan.items[0].format, Format::ChdCd);
    assert!(
        plan.items[0].sidecar.is_none(),
        "a CHD alone, no folder beside it"
    );
    assert!(
        plan.skipped
            .iter()
            .any(|(t, why)| t == "Cube (USA)" && why.contains("CHD")),
        "{:?}",
        plan.skipped
    );
    let report = lib.convert_apply(None, |_, _| {}).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(d.join(format!("{game}.chd")).exists());
    assert!(!d.join(format!("{game}.cue")).exists(), "originals to the trash");
    assert!(d.join("cube.iso").exists(), "GameCube left alone");
    let v = lib.library_view().unwrap();
    assert!(
        v.rows
            .iter()
            .any(|r| r.title == game && r.status == RowStatus::Verified),
        "the CHD verifies"
    );
    // Nothing more to convert.
    assert!(lib.convert_plan(None).unwrap().items.is_empty());
}
