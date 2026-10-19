//! Writes that never lose a file: folder games, undo, arcade sets and
//! games half in Games/. Always on copies in temp folders.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use romforge_core::cleanup::Format;
use romforge_core::home::Home;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

mod common;
use common::home::{files_under, write_multi_dat};

fn md5_hex(data: &[u8]) -> String {
    romforge_core::hash::hash_stream(data, data.len() as u64)
        .unwrap()
        .raw
        .md5
}

/// A ScummVM game in `incoming/Steel Sky`, scanned and planned for import.
fn scummvm_game(tmp: &Path) -> (Home, Library, PathBuf, romforge_core::cleanup::CleanupPlan) {
    let home = Home::create(tmp).unwrap();
    let incoming = tmp.join("incoming");
    let game = incoming.join("Steel Sky");
    std::fs::create_dir_all(&game).unwrap();
    let dnr: Vec<u8> = (0..80u8).collect();
    let dsk: Vec<u8> = (0..9000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(game.join("SKY.DNR"), &dnr).unwrap();
    std::fs::write(game.join("SKY.DSK"), &dsk).unwrap();
    let list = format!(
        "scummvm (\n\tversion \"2.9.0\"\n)\n\ngame (\n\tname \"sky\"\n\ttitle \"Beneath a Steel Sky\"\n\
         \tplatform \"pc\"\n\tengine \"sky\"\n\trom ( name \"sky.dnr\" size 80 md5-5000 {} )\n\
         \trom ( name \"sky.dsk\" size -1 md5-5000 t:{} )\n)\n",
        md5_hex(&dnr),
        md5_hex(&dsk[dsk.len() - 5000..])
    );
    let mut lib = Library::open(home.db()).unwrap();
    lib.scummvm_import(list.as_bytes()).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let plan = lib.import_plan(Some(&incoming)).unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    assert_eq!(plan.items[0].format, Format::Dir);
    (home, lib, game, plan)
}

/// A folder game copied as a folder goes to the trash whole: a file added
/// since the plan stops it, and nothing is written or trashed.
#[test]
fn a_folder_game_changed_since_the_plan_stays() {
    let tmp = tempfile::tempdir().unwrap();
    let (_home, mut lib, game, plan) = scummvm_game(tmp.path());
    std::fs::write(game.join("SAVE.000"), b"my save").unwrap();
    let before = files_under(&game);
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert_eq!(r.failed.len(), 1, "{r:?}");
    assert!(r.failed[0].1.contains("changed since the plan"), "{r:?}");
    assert_eq!(files_under(&game), before, "the folder is untouched");
    assert!(!plan.items[0].target.exists(), "nothing written");
}

/// Undo removes what the batch wrote into a folder game, and keeps what
/// was added there since (a save), never deleting it.
#[test]
fn undo_keeps_files_added_to_a_folder_game() {
    let tmp = tempfile::tempdir().unwrap();
    let (_home, mut lib, game, plan) = scummvm_game(tmp.path());
    let before = files_under(&game);
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    let target = &plan.items[0].target;
    std::fs::create_dir_all(target.join("saves")).unwrap();
    std::fs::write(target.join("saves/sky.000"), b"my save").unwrap();
    let u = lib.cleanup_undo(None).unwrap();
    assert!(u.failed.is_empty(), "{u:?}");
    assert_eq!(files_under(&game), before, "the game is back");
    assert_eq!(files_under(target), ["saves/sky.000"], "the save stays, alone");
}

/// Undo of a batch that isn't there, or was undone already, is an error.
#[test]
fn undo_names_a_batch_that_can_be_undone() {
    let tmp = tempfile::tempdir().unwrap();
    let (_home, mut lib, _game, plan) = scummvm_game(tmp.path());
    let e = lib.cleanup_undo(Some(4242)).unwrap_err();
    assert!(e.to_string().contains("no batch #4242"), "{e}");
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    lib.cleanup_undo(Some(r.batch_id)).unwrap();
    let e = lib.cleanup_undo(Some(r.batch_id)).unwrap_err();
    assert!(e.to_string().contains("undone already"), "{e}");
}

fn zip(path: &Path, members: &[(&str, &[u8])]) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, data) in members {
        z.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap();
}

/// An arcade set's zip that appears in Games/ after the plan, with other
/// contents: the set fails and its original isn't trashed.
#[test]
fn an_arcade_zip_that_differs_keeps_its_original() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("arcade");
    std::fs::create_dir_all(&incoming).unwrap();
    let p1: Vec<u8> = (0..4096u32).map(|i| (i % 253) as u8).collect();
    let bios = vec![1u8; 1024];
    zip(&incoming.join("solo.zip"), &[("solo.p1", &p1)]);
    zip(&incoming.join("zbios.zip"), &[("z.rom", &bios)]);
    let rom = |name: &str, d: &[u8], extra: &str| {
        let h = romforge_core::hash::hash_stream(d, d.len() as u64).unwrap().raw;
        format!(
            "<rom name=\"{name}\" size=\"{}\" crc=\"{}\" sha1=\"{}\"{extra}/>",
            d.len(),
            h.crc32,
            h.sha1
        )
    };
    let xml = format!(
        "<?xml version=\"1.0\"?>\n<mame build=\"0.289 (mame0289)\">\n\
         <machine name=\"zbios\" isbios=\"yes\"><description>Z BIOS</description>{}</machine>\n\
         <machine name=\"solo\" romof=\"zbios\"><description>Solo</description>{}{}</machine>\n</mame>\n",
        rom("z.rom", &bios, ""),
        rom("z.rom", &bios, " merge=\"z.rom\""),
        rom("solo.p1", &p1, ""),
    );
    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat_bytes(xml.as_bytes(), "mame -listxml (0.289)", Some("Arcade"))
        .unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let plan = lib.import_plan(Some(&incoming)).unwrap();
    // The game's item brings its BIOS's zip too, not as its target.
    let solo = plan.items.iter().find(|i| i.title == "Solo").expect("planned");
    assert!(solo.parts.iter().any(|p| p.name == "zbios.zip"), "{plan:?}");
    let dest = home.games().join("Arcade/zbios.zip");
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    zip(&dest, &[("other.p1", b"someone else's")]);
    let r = lib.cleanup_apply(&plan, |_, _| {}).unwrap();
    assert_eq!(r.failed.len(), 1, "{r:?}");
    assert!(r.failed[0].1.contains("differs"), "{r:?}");
    assert!(
        incoming.join("solo.zip").is_file() && incoming.join("zbios.zip").is_file(),
        "the originals stay"
    );
    assert_eq!(r.trashed, 0);
}

/// A game counts as already in Games/ only when every ROM of it is there:
/// one ROM there doesn't make the copies outside redundant.
#[test]
fn half_a_game_in_games_is_not_the_game() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let (a, b) = (vec![3u8; 2048], vec![4u8; 4096]);
    let dat = tmp.path().join("two.dat");
    write_multi_dat(
        &dat,
        &[("Two (USA)", vec![("two-a.gb", &a[..]), ("two-b.gb", &b[..])])],
    );
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    std::fs::write(incoming.join("two-a.gb"), &a).unwrap();
    std::fs::write(incoming.join("two-b.gb"), &b).unwrap();
    let there = home.games().join("Game Boy");
    std::fs::create_dir_all(&there).unwrap();
    std::fs::write(there.join("two-a.gb"), &a).unwrap();
    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(&dat, Some("Game Boy")).unwrap();
    lib.scan(home.games(), Scope::All).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let rows = lib.library_view().unwrap().rows;
    assert!(
        rows.iter()
            .any(|r| r.title == "Two (USA)" && r.status == RowStatus::Verified),
        "{rows:?}"
    );
    let plan = lib.import_plan(Some(&incoming)).unwrap();
    let item = plan
        .items
        .iter()
        .find(|i| i.title == "Two (USA)")
        .expect("planned");
    assert!(!item.existing, "{item:?}");
    assert_eq!(item.parts.len(), 2, "both ROMs written: {item:?}");
}
