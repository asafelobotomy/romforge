//! Milestone 19: patching a game into a new copy, the original untouched.
//! Always on copies in temp folders.
//!
//! Cartridges.

use std::io::{Read, Write as _};
use std::path::Path;

use romforge_core::home::Home;
use romforge_core::view::{Relation, RowStatus};
use romforge_core::{Library, Scope};

mod common;
use common::fixtures;
use common::patcher::*;

/// A headered NES ROM: an IPS is applied to the DAT's form (no iNES
/// header), the header goes back on, the original stays as it was, and
/// the copy lists as a translation of the game; undo removes it.
#[test]
fn an_ips_makes_a_translated_copy() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let rom = incoming.join("Test NES (iNES).nes");
    std::fs::copy(fixtures().join("roms/nes/Test NES (iNES).nes"), &rom).unwrap();
    let original = std::fs::read(&rom).unwrap();
    let patch = tmp.path().join("Test NES English v1.0.ips");
    std::fs::write(&patch, ips(0, b"HELLO")).unwrap();

    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(fixtures().join("dats/nes.dat"), None).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let base = row_id(&lib, "Test NES (iNES)");
    let plan = lib
        .patch_plan(&base, &patch, Some(Relation::Translation), Some("English"))
        .unwrap();
    assert_eq!(plan.title, "Test NES (iNES) [Translation: English]");
    assert!(!plan.disc);
    assert!(plan.applies_to.contains("the DAT's form"), "{plan:?}");
    assert!(plan.notes.iter().any(|n| n.contains("says nothing")), "{plan:?}");
    let target = home
        .games()
        .join("NES/Test NES (iNES) [Translation- English].zip");
    assert_eq!(plan.target, target, "a file name safe everywhere (':' isn't)");
    assert!(!target.exists(), "planning writes nothing");

    let done = lib.patch_apply(&plan).unwrap();
    assert_eq!(
        std::fs::read(&rom).unwrap(),
        original,
        "the original is untouched"
    );
    let mut z = zip::ZipArchive::new(std::fs::File::open(&target).unwrap()).unwrap();
    let mut patched = Vec::new();
    z.by_name("Test NES (iNES) [Translation- English].nes")
        .unwrap()
        .read_to_end(&mut patched)
        .unwrap();
    assert_eq!(&patched[..16], &original[..16], "the iNES header back on");
    assert_eq!(&patched[16..21], b"HELLO");
    assert_eq!(&patched[21..], &original[21..]);
    assert_eq!(done.sha1, sha1(&patched));
    assert!(
        z.by_name("romforge/Test NES (iNES) [Translation- English].romforge.md")
            .is_ok()
    );

    let rows = lib.library_view().unwrap().rows;
    let copy = rows.iter().find(|r| r.title == plan.title).expect("listed");
    assert_eq!(copy.status, RowStatus::Derivative);
    let base_row = rows.iter().find(|r| r.title == "Test NES (iNES)").unwrap();
    assert!(
        copy.group.is_some() && copy.group == base_row.group,
        "a version of the game"
    );
    let d = lib.game_detail(&copy.id).unwrap();
    let made = d.patched.expect("what made it");
    assert_eq!(
        (made.patch.as_str(), made.kind.as_str()),
        ("Test NES English v1.0.ips", "IPS")
    );

    // Patching again into the same name is refused; undo removes the copy.
    assert!(
        lib.patch_plan(&base, &patch, Some(Relation::Translation), Some("English"))
            .is_err()
    );
    lib.cleanup_undo(None).unwrap();
    assert!(!target.exists());
    assert_eq!(std::fs::read(&rom).unwrap(), original);
}

/// A BPS names the ROM it was made from: another revision is refused.
#[test]
fn a_bps_for_another_rom_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    std::fs::copy(fixtures().join("roms/gb/Test GB.gb"), incoming.join("Test GB.gb")).unwrap();
    let data = std::fs::read(incoming.join("Test GB.gb")).unwrap();
    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let base = row_id(&lib, "Test GB");

    let wrong = tmp.path().join("other.bps");
    std::fs::write(&wrong, bps(data.len() as u64, 0x1234_5678, &data)).unwrap();
    let err = lib
        .patch_plan(&base, &wrong, Some(Relation::Hack), None)
        .unwrap_err()
        .to_string();
    assert!(err.contains("made for a ROM"), "{err}");

    let mut target = data.clone();
    target[0] ^= 0xff;
    let right = tmp.path().join("Hard Mode.bps");
    std::fs::write(&right, bps(data.len() as u64, crc32fast::hash(&data), &target)).unwrap();
    let plan = lib.patch_plan(&base, &right, Some(Relation::Hack), None).unwrap();
    assert_eq!(
        plan.title, "Test GB [Hack: Hard Mode]",
        "the tag from the patch's name"
    );
    assert!(plan.applies_to.contains("as it is"));
    let done = lib.patch_apply(&plan).unwrap();
    assert_eq!(done.sha1, sha1(&target));
}

/// A cartridge game of two ROMs in a zip: a BPS goes on the one it names,
/// an IPS (which names none) on the bigger; the other ROM comes through,
/// both under the DAT's names.
#[test]
fn a_game_of_two_roms_patches_the_right_one() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let (prg, chr) = (pattern(7, 4096), pattern(9, 1024));
    let rom = |name: &str, d: &[u8]| {
        let h = romforge_core::hash::hash_stream(d, d.len() as u64).unwrap().raw;
        format!(
            "<rom name=\"{name}\" size=\"{}\" crc=\"{}\" md5=\"{}\" sha1=\"{}\"/>",
            d.len(),
            h.crc32,
            h.md5,
            h.sha1
        )
    };
    let dat = tmp.path().join("gb.dat");
    std::fs::write(
        &dat,
        format!(
            "<?xml version=\"1.0\"?><datafile><header><name>Nintendo - Game Boy</name></header>\
             <game name=\"Two Chips (Japan)\">{}{}</game></datafile>",
            rom("chr.gb", &chr),
            rom("prg.gb", &prg)
        ),
    )
    .unwrap();
    let mut z = zip::ZipWriter::new(std::fs::File::create(incoming.join("Two Chips (Japan).zip")).unwrap());
    for (n, d) in [("chr.gb", &chr), ("prg.gb", &prg)] {
        z.start_file(n, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(d).unwrap();
    }
    z.finish().unwrap();
    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let base = row_id(&lib, "Two Chips (Japan)");

    let read = |path: &Path, name: &str| {
        let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        let mut d = Vec::new();
        z.by_name(name).unwrap().read_to_end(&mut d).unwrap();
        d
    };
    let mut new_chr = chr.clone();
    new_chr[0..3].copy_from_slice(b"ABC");
    let p = tmp.path().join("Font.bps");
    std::fs::write(&p, bps(chr.len() as u64, crc32fast::hash(&chr), &new_chr)).unwrap();
    let plan = lib
        .patch_plan(&base, &p, Some(Relation::Translation), Some("English"))
        .unwrap();
    assert!(plan.applies_to.starts_with("chr.gb"), "{plan:?}");
    let done = lib.patch_apply(&plan).unwrap();
    assert_eq!(read(&done.target, "chr.gb"), new_chr);
    assert_eq!(read(&done.target, "prg.gb"), prg, "the other ROM kept");
    assert_eq!(done.sha1, sha1(&new_chr));

    let p = tmp.path().join("Hard.ips");
    std::fs::write(&p, ips(10, b"HARD")).unwrap();
    let plan = lib.patch_plan(&base, &p, Some(Relation::Hack), None).unwrap();
    assert!(plan.applies_to.starts_with("prg.gb"), "{plan:?}");
    assert!(plan.notes.iter().any(|n| n.contains("the biggest")), "{plan:?}");
    let done = lib.patch_apply(&plan).unwrap();
    assert_eq!(&read(&done.target, "prg.gb")[10..14], b"HARD");
    assert_eq!(read(&done.target, "chr.gb"), chr);
    let rows = lib.library_view().unwrap().rows;
    assert!(
        rows.iter()
            .any(|r| r.title == plan.title && r.status == RowStatus::Derivative),
        "{rows:#?}"
    );
}
