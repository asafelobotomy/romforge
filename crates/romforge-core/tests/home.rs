//! Milestone 13: one RomForge folder. Always on copies in temp folders.

use std::path::{Path, PathBuf};

use romforge_core::home::Home;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn files_under(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().strip_prefix(dir).unwrap().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

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
    let rows = lib.library_view().unwrap().rows;
    let gb = rows.iter().find(|r| r.title == "Test GB").unwrap();
    assert_eq!(gb.status, RowStatus::Verified, "found again in Games by refresh");

    // Nothing left to import from there; the RomForge folder itself never is.
    assert!(lib.import_plan(Some(&incoming)).unwrap().items.is_empty());
    assert!(lib.import_plan(Some(&home.games())).is_err());

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
