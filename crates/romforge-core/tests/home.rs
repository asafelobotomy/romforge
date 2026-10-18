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

/// Import two disks of a set on `system` (images named `.<ext>`); returns
/// the RomForge folder's Games and the report.
fn import_set(system: &str, ext: &str) -> (tempfile::TempDir, Home, romforge_core::cleanup::CleanupReport) {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let disks: Vec<Vec<u8>> = (1u8..=2).map(|n| vec![n; 4096]).collect();
    let games: Vec<(String, String)> = (1..=2)
        .map(|n| {
            (
                format!("Ys (1987)(Nihon Falcom)(Disk {n} of 2)"),
                format!("Ys (1987)(Nihon Falcom)(Disk {n} of 2).{ext}"),
            )
        })
        .collect();
    let rows: Vec<(&str, &str, &[u8])> = games
        .iter()
        .zip(&disks)
        .map(|((g, r), d)| (g.as_str(), r.as_str(), d.as_slice()))
        .collect();
    let dat = tmp.path().join("set.dat");
    write_dat(&dat, "A TOSEC set", &rows);
    for ((_, rom), data) in games.iter().zip(&disks) {
        std::fs::write(incoming.join(rom), data).unwrap();
    }
    let before = files_under(&incoming);
    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(&dat, Some(system)).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let plan = lib.import_plan(Some(&incoming)).unwrap();
    assert_eq!(plan.items.len(), 2, "{plan:?}");
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    assert!(files_under(&incoming).is_empty());
    // Undo is checked on a second library over the same folder below.
    let report = r.clone();
    let games_dir = home.games();
    let listed = files_under(&games_dir);
    lib.cleanup_undo(None).unwrap();
    assert_eq!(files_under(&incoming), before, "undo puts the disks back");
    assert!(
        files_under(&games_dir).is_empty(),
        "undo removes what import wrote"
    );
    assert!(
        std::fs::read_dir(games_dir.join(system)).map_or(true, |d| d
            .filter_map(Result::ok)
            .all(|e| !e.file_name().to_string_lossy().starts_with("Ys"))),
        "and the set's folder"
    );
    // Apply again, to look at what it writes.
    lib.scan(&incoming, Scope::All).unwrap();
    let plan = lib.import_plan(Some(&incoming)).unwrap();
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert_eq!(files_under(&games_dir), listed);
    assert_eq!(r.playlists, report.playlists);
    (tmp, home, r)
}

/// A set of a system whose core opens disks itself (PC-98's np2kai) is
/// kept unzipped in its own folder, with the `.cmd` np2kai reads.
#[test]
fn a_pc98_set_is_kept_loose_with_its_cmd() {
    let (_tmp, home, r) = import_set("PC-98", "d88");
    assert_eq!(r.playlists, 1);
    let set = home.games().join("PC-98/Ys (1987)(Nihon Falcom)");
    assert!(set.join("Ys (1987)(Nihon Falcom)(Disk 1 of 2).d88").is_file());
    assert!(set.join("Ys (1987)(Nihon Falcom)(Disk 2 of 2).d88").is_file());
    assert_eq!(
        std::fs::read_to_string(set.join("Ys (1987)(Nihon Falcom).cmd")).unwrap(),
        "np2kai \"Ys (1987)(Nihon Falcom)(Disk 1 of 2).d88\" \"Ys (1987)(Nihon Falcom)(Disk 2 of 2).d88\"\n"
    );
    assert!(set.join("romforge/Ys (1987)(Nihon Falcom)(Disk 1 of 2)").is_dir());

    // Play starts the set's .cmd, whichever disk's row it is.
    let mut lib = Library::open(home.db()).unwrap();
    lib.refresh(romforge_core::scan::ScanOptions::default(), |_, _| {})
        .unwrap();
    let rows = lib.library_view().unwrap().rows;
    let disk2 = rows
        .iter()
        .find(|r| r.title == "Ys (1987)(Nihon Falcom)(Disk 2 of 2)")
        .unwrap();
    assert_eq!(disk2.status, RowStatus::Verified);
    let bin = _tmp.path().join("bin");
    let cores = _tmp.path().join("cores");
    for (dir, name) in [
        (&bin, "retroarch".to_owned()),
        (
            &cores,
            format!("np2kai_libretro.{}", std::env::consts::DLL_EXTENSION),
        ),
    ] {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(&name), "#!/bin/sh\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.join(&name), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let env = romforge_core::launch::Env {
        path: vec![bin],
        core_dirs: vec![cores],
        flatpak_dirs: vec![],
    };
    let spec = lib.launch_row(&env, &disk2.id, None).unwrap();
    assert_eq!(spec.rom, set.join("Ys (1987)(Nihon Falcom).cmd"));
}

/// PC-88's quasi88 reads an `.m3u` of the loose images.
#[test]
fn a_pc88_set_is_kept_loose_with_its_m3u() {
    let (_tmp, home, r) = import_set("PC-88", "d88");
    assert_eq!(r.playlists, 1);
    let set = home.games().join("PC-88/Ys (1987)(Nihon Falcom)");
    assert_eq!(
        std::fs::read_to_string(set.join("Ys (1987)(Nihon Falcom).m3u")).unwrap(),
        "Ys (1987)(Nihon Falcom)(Disk 1 of 2).d88\nYs (1987)(Nihon Falcom)(Disk 2 of 2).d88\n"
    );
}

/// Elsewhere every disk is a zip, and the playlist lists the zips.
#[test]
fn an_amiga_set_is_zipped_with_an_m3u() {
    let (_tmp, home, r) = import_set("Amiga", "adf");
    assert_eq!(r.playlists, 1);
    assert_eq!(
        std::fs::read_to_string(home.games().join("Amiga/Ys (1987)(Nihon Falcom).m3u")).unwrap(),
        "Ys (1987)(Nihon Falcom)(Disk 1 of 2).zip\nYs (1987)(Nihon Falcom)(Disk 2 of 2).zip\n"
    );
}
