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

/// A Logiqx DAT for `system` with one ROM per game: (game, rom name, data).
fn write_dat(path: &Path, name: &str, games: &[(&str, &str, &[u8])]) {
    let mut xml = format!("<?xml version=\"1.0\"?>\n<datafile><header><name>{name}</name></header>\n");
    for (game, rom, data) in games {
        let h = romforge_core::hash::hash_stream(*data, data.len() as u64)
            .unwrap()
            .raw;
        xml += &format!(
            "<game name=\"{game}\"><rom name=\"{rom}\" size=\"{}\" crc=\"{}\" sha1=\"{}\"/></game>\n",
            data.len(),
            h.crc32,
            h.sha1
        );
    }
    xml += "</datafile>\n";
    std::fs::write(path, xml).unwrap();
}

/// Importing every disk of a set writes its playlist; undo removes it.
#[test]
fn import_writes_a_playlist_for_a_complete_disk_set() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let disks: Vec<Vec<u8>> = (1u8..=2).map(|n| vec![n; 4096]).collect();
    let games: Vec<(String, String)> = (1..=2)
        .map(|n| {
            (
                format!("Ys (1987)(Nihon Falcom)(Disk {n} of 2)"),
                format!("Ys (1987)(Nihon Falcom)(Disk {n} of 2).d88"),
            )
        })
        .collect();
    let rows: Vec<(&str, &str, &[u8])> = games
        .iter()
        .zip(&disks)
        .map(|((g, r), d)| (g.as_str(), r.as_str(), d.as_slice()))
        .collect();
    let dat = tmp.path().join("pc98.dat");
    write_dat(&dat, "NEC PC-9801 - Games - [D88]", &rows);
    for ((_, rom), data) in games.iter().zip(&disks) {
        std::fs::write(incoming.join(rom), data).unwrap();
    }

    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(&dat, Some("PC-98")).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let plan = lib.import_plan(Some(&incoming)).unwrap();
    assert_eq!(plan.items.len(), 2, "{plan:?}");
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    assert_eq!(r.playlists, 1);
    let list = home.games().join("PC-98/Ys (1987)(Nihon Falcom).m3u");
    assert_eq!(
        std::fs::read_to_string(&list).unwrap(),
        "Ys (1987)(Nihon Falcom)(Disk 1 of 2).zip\nYs (1987)(Nihon Falcom)(Disk 2 of 2).zip\n"
    );
    lib.cleanup_undo(None).unwrap();
    assert!(!list.exists());
    assert_eq!(files_under(&home.games()), Vec::<String>::new());
}
