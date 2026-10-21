//! ScummVM games still in their zip or 7z: named by ScummVM's list without
//! unpacking, listed as one game (not a row per file), not playable until
//! import unpacks them into a folder; undo puts the archive back. Always on
//! copies in temp folders.

use std::io::Write as _;
use std::path::Path;

use romforge_core::cleanup::Format;
use romforge_core::home::Home;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

fn md5_hex(data: &[u8]) -> String {
    romforge_core::hash::hash_stream(data, data.len() as u64)
        .unwrap()
        .raw
        .md5
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

/// Beneath a Steel Sky's two listed files (one by its first bytes, one by
/// its last), a file the list doesn't name, and the list itself.
fn sky() -> (Vec<(&'static str, Vec<u8>)>, String) {
    let dnr: Vec<u8> = (0..80u8).collect();
    let dsk: Vec<u8> = (0..9000u32).map(|i| (i % 251) as u8).collect();
    let list = format!(
        "scummvm (\n\tversion \"2.9.0\"\n)\n\ngame (\n\tname \"sky\"\n\ttitle \"Beneath a Steel Sky\"\n\textra \"CD\"\n\tlanguage \"en\"\n\tplatform \"pc\"\n\trom ( name \"sky.dnr\" size 80 md5-5000 {} )\n\trom ( name \"sky.dsk\" size -1 md5-5000 t:{} )\n)\n",
        md5_hex(&dnr),
        md5_hex(&dsk[dsk.len() - 5000..])
    );
    let files = vec![
        ("Steel Sky/SKY.DNR", dnr),
        ("Steel Sky/SKY.DSK", dsk),
        ("Steel Sky/ART/ROOM1.IMG", vec![7u8; 300]),
    ];
    (files, list)
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

/// Writes an archive of (name, data).
type Writer = fn(&Path, &[(&str, Vec<u8>)]);

fn round_trip(name: &str, write: Writer) {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let (files, list) = sky();
    let archive = incoming.join(name);
    write(&archive, &files);
    // The same names with other data: not the game.
    let mut wrong = files.clone();
    wrong[0].1 = vec![1u8; 80];
    write(&incoming.join(format!("not {name}")), &wrong);
    let before = files_under(&incoming);

    let mut lib = Library::open(home.db()).unwrap();
    lib.scummvm_import(list.as_bytes()).unwrap();
    let scanned = lib.scan(&incoming, Scope::All).unwrap();
    // The archive is the game by the list: its members aren't hashed.
    assert_eq!(scanned.in_games, files.len(), "{scanned:?}");
    let rows = lib.library_view().unwrap().rows;
    let found: Vec<_> = rows.iter().filter(|r| r.status == RowStatus::Detected).collect();
    assert_eq!(found.len(), 1, "{rows:?}");
    let sky = found[0];
    assert_eq!(sky.title, "Beneath a Steel Sky (CD) (pc) (en)");
    assert_eq!(sky.folder.as_deref(), Some(archive.to_str().unwrap()));
    assert!(sky.packed && !sky.playable, "{sky:?}");
    // Its files are the game's, not rows of their own: what is left
    // unknown is the other archive's.
    for r in rows.iter().filter(|r| r.status == RowStatus::Unknown) {
        for f in lib.game_detail(&r.id).unwrap().files {
            assert!(f.path.contains(&format!("not {name}")), "{}", f.path);
        }
    }

    let plan = lib.import_plan(Some(&incoming)).unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    let target = home.games().join("ScummVM/Beneath a Steel Sky (CD) (pc) (en)");
    assert_eq!(
        (plan.items[0].format, &plan.items[0].target),
        (Format::Dir, &target)
    );
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    let unpacked = files_under(&target);
    for n in ["SKY.DNR", "SKY.DSK", "ART/ROOM1.IMG", "sky.scummvm"] {
        assert!(unpacked.contains(&n.to_owned()), "{unpacked:?}");
    }
    assert_eq!(std::fs::read(target.join("SKY.DSK")).unwrap(), files[1].1);
    assert!(!archive.exists(), "the archive went to the trash");

    let rows = lib.library_view().unwrap().rows;
    let sky = rows.iter().find(|r| r.status == RowStatus::Detected).unwrap();
    assert_eq!(sky.folder.as_deref(), Some(target.to_str().unwrap()));
    assert!(!sky.packed && sky.playable);

    lib.cleanup_undo(None).unwrap();
    assert_eq!(files_under(&incoming), before);
    assert!(!target.exists());
}

#[test]
fn a_scummvm_game_in_a_zip() {
    round_trip("Steel Sky.zip", zip);
}

#[test]
fn a_scummvm_game_in_a_7z() {
    round_trip("Steel Sky.7z", seven_z);
}
