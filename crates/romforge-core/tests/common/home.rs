//! Helpers from the `home` tests.

use std::path::Path;

use romforge_core::home::Home;
use romforge_core::{Library, Scope};

pub fn files_under(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().strip_prefix(dir).unwrap().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// A Logiqx DAT for `system` with one ROM per game: (game, rom name, data).
pub fn write_dat(path: &Path, name: &str, games: &[(&str, &str, &[u8])]) {
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
pub fn import_set(
    system: &str,
    ext: &str,
) -> (tempfile::TempDir, Home, romforge_core::cleanup::CleanupReport) {
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

/// A game of a test DAT: its name and its ROMs (name, data).
pub type DatGame<'a> = (&'a str, Vec<(&'a str, &'a [u8])>);

/// A Logiqx DAT whose games have several ROMs.
pub fn write_multi_dat(path: &Path, games: &[DatGame]) {
    let mut xml = "<?xml version=\"1.0\"?>\n<datafile><header><name>Discs</name></header>\n".to_owned();
    for (game, roms) in games {
        xml += &format!("<game name=\"{game}\">");
        for (rom, data) in roms {
            let h = romforge_core::hash::hash_stream(*data, data.len() as u64)
                .unwrap()
                .raw;
            xml += &format!(
                "<rom name=\"{rom}\" size=\"{}\" crc=\"{}\" sha1=\"{}\"/>",
                data.len(),
                h.crc32,
                h.sha1
            );
        }
        xml += "</game>\n";
    }
    xml += "</datafile>\n";
    std::fs::write(path, xml).unwrap();
}
