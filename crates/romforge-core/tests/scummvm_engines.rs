//! ScummVM games of the engines its list leaves out (SCUMM, SCI), still in
//! their zip or 7z: recognised by their files without unpacking, one game
//! each (not a row per file), named by the index file's game id when
//! libretro's names know it; import unpacks them with their `.scummvm`
//! file. Always on copies in temp folders.

use std::io::Write as _;
use std::path::Path;

use romforge_core::cleanup::Format;
use romforge_core::home::Home;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

/// Day of the Tentacle's index (XORed "RNAM" block) and data file, and a
/// program beside them, in a folder.
fn dott() -> Vec<(&'static str, Vec<u8>)> {
    let mut index = b"RNAM\0\0\0\x20".to_vec();
    index.resize(4000, 0);
    index.iter_mut().for_each(|b| *b ^= 0x69);
    vec![
        ("DOTT/TENTACLE.000", index),
        ("DOTT/TENTACLE.001", vec![3u8; 9000]),
        ("DOTT/DOTT.EXE", vec![4u8; 500]),
    ]
}

/// A Sierra SCI game: its resource map and volumes.
fn sci() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("RESOURCE.MAP", vec![1u8; 300]),
        ("RESOURCE.000", vec![2u8; 7000]),
        ("RESOURCE.001", vec![5u8; 7000]),
        ("SIERRA.EXE", vec![6u8; 400]),
    ]
}

fn zip(path: &Path, files: &[(&str, Vec<u8>)]) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, data) in files {
        z.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap();
}

fn seven_z(path: &Path, files: &[(&str, Vec<u8>)]) {
    let mut sz = sevenz_rust2::ArchiveWriter::create(path).unwrap();
    sz.push_archive_entries(
        files
            .iter()
            .map(|(n, _)| sevenz_rust2::ArchiveEntry::new_file(n))
            .collect(),
        files
            .iter()
            .map(|(_, d)| sevenz_rust2::SourceReader::new(&d[..]))
            .collect(),
    )
    .unwrap();
    sz.finish().unwrap();
}

#[test]
fn scumm_and_sci_games_in_archives_are_one_game_each() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let (dott_zip, kq_7z) = (
        incoming.join("Day of the Tentacle.zip"),
        incoming.join("King's Quest IV.7z"),
    );
    zip(&dott_zip, &dott());
    seven_z(&kq_7z, &sci());
    // The same names, but not a SCUMM index: no game.
    let mut parts = dott();
    parts[0].1 = vec![0u8; 4000];
    zip(&incoming.join("parts.zip"), &parts);

    let mut lib = Library::open(home.db()).unwrap();
    // libretro's names: SCUMM's index file names the game.
    lib.scummvm_titles(b"clrmamepro (\n\tname \"ScummVM\"\n)\ngame (\n\tname \"Day of the Tentacle\"\n\tcode \"tentacle\"\n\trom ( name \"tentacle.scummvm\" size 8 crc 00000000 )\n)\n")
        .unwrap();
    let scanned = lib.scan(&incoming, Scope::All).unwrap();
    assert_eq!(
        scanned.in_games, 7,
        "their members aren't hashed one by one: {scanned:?}"
    );
    let rows = lib.library_view().unwrap().rows;
    let mut found: Vec<_> = rows
        .iter()
        .filter(|r| r.status == RowStatus::Detected)
        .map(|r| (r.title.as_str(), r.folder.as_deref(), r.packed, r.playable))
        .collect();
    found.sort();
    assert_eq!(
        found,
        [
            ("Day of the Tentacle", dott_zip.to_str(), true, false),
            ("King's Quest IV", kq_7z.to_str(), true, false),
        ],
        "{rows:?}"
    );
    for r in rows.iter().filter(|r| r.status == RowStatus::Unknown) {
        for f in lib.game_detail(&r.id).unwrap().files {
            assert!(f.path.contains("parts.zip"), "{}", f.path);
        }
    }

    // Import unpacks it, with the game id's `.scummvm` file; it stays one
    // game as a folder.
    let plan = lib.import_plan(Some(&incoming)).unwrap();
    let target = home.games().join("ScummVM/Day of the Tentacle");
    let item = plan
        .items
        .iter()
        .find(|i| i.target == target)
        .expect("DOTT planned");
    assert_eq!(item.format, Format::Dir);
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    assert!(target.join("tentacle.scummvm").is_file());
    assert!(target.join("TENTACLE.000").is_file());
    let rows = lib.library_view().unwrap().rows;
    let dott = rows.iter().find(|r| r.title == "Day of the Tentacle").unwrap();
    assert_eq!(dott.folder.as_deref(), target.to_str());
    assert!(!dott.packed && dott.playable, "{dott:?}");
    // SCI's has no id yet: no `.scummvm` file (ScummVM's --detect names it).
    let kq = home.games().join("ScummVM/King's Quest IV");
    assert!(kq.join("RESOURCE.MAP").is_file());
    assert!(
        !walkdir::WalkDir::new(&kq)
            .into_iter()
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().ends_with(".scummvm"))
    );
}
