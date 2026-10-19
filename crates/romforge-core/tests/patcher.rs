//! Milestone 19: patching a game into a new copy, the original untouched.
//! Always on copies in temp folders.

use std::io::{Read, Write as _};
use std::path::{Path, PathBuf};

use chd::metadata::KnownMetadata;
use chd::write::{ChdWriteOptions, ChdWriter};
use romforge_core::home::Home;
use romforge_core::view::{Relation, RowStatus};
use romforge_core::{Library, Scope};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn sha1(d: &[u8]) -> String {
    romforge_core::hash::hash_stream(d, d.len() as u64)
        .unwrap()
        .raw
        .sha1
}

/// An IPS writing `data` at `at`.
fn ips(at: u32, data: &[u8]) -> Vec<u8> {
    let mut p = b"PATCH".to_vec();
    p.extend_from_slice(&at.to_be_bytes()[1..]);
    p.extend_from_slice(&(data.len() as u16).to_be_bytes());
    p.extend_from_slice(data);
    p.extend_from_slice(b"EOF");
    p
}

fn num(mut n: u64, out: &mut Vec<u8>) {
    loop {
        let x = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(0x80 | x);
            return;
        }
        out.push(x);
        n -= 1;
    }
}

/// A BPS that writes `target` whole (one TargetRead), made from a source
/// of `source_size` bytes with CRC32 `source_crc`.
fn bps(source_size: u64, source_crc: u32, target: &[u8]) -> Vec<u8> {
    let mut p = b"BPS1".to_vec();
    num(source_size, &mut p);
    num(target.len() as u64, &mut p);
    num(0, &mut p);
    num(((target.len() as u64 - 1) << 2) | 1, &mut p);
    p.extend_from_slice(target);
    p.extend_from_slice(&source_crc.to_le_bytes());
    p.extend_from_slice(&crc32fast::hash(target).to_le_bytes());
    let crc = crc32fast::hash(&p);
    p.extend_from_slice(&crc.to_le_bytes());
    p
}

fn row_id(lib: &Library, title: &str) -> String {
    lib.library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.title == title)
        .unwrap_or_else(|| panic!("no row {title}"))
        .id
}

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

fn pattern(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

/// A two-track CD as a CHD (data, then audio with a 2-frame pregap not
/// stored); returns the data and audio `.bin`s as Redump has them.
fn write_cd(path: &Path) -> (Vec<u8>, Vec<u8>) {
    let (data_frames, audio_frames) = (10usize, 7usize);
    let mut data_bin = pattern(1, data_frames * 2352);
    data_bin[..12].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0]);
    data_bin[15] = 1;
    let audio_bin = pattern(2, audio_frames * 2352);
    let mut frames: Vec<u8> = Vec::new();
    let mut push = |bin: &[u8], audio: bool, n: usize| {
        for s in bin.chunks(2352) {
            if audio {
                for pair in s.chunks(2) {
                    frames.extend_from_slice(&[pair[1], pair[0]]);
                }
            } else {
                frames.extend_from_slice(s);
            }
            frames.extend_from_slice(&[0u8; 96]);
        }
        for _ in 0..(4 - n % 4) % 4 {
            frames.extend_from_slice(&[0u8; 2448]);
        }
    };
    push(&data_bin, false, data_frames);
    push(&audio_bin, true, audio_frames);
    let mut w = ChdWriter::new(std::fs::File::create(path).unwrap(), ChdWriteOptions::cd()).unwrap();
    w.write_all(&frames).unwrap();
    for meta in [
        format!(
            "TRACK:1 TYPE:MODE1_RAW SUBTYPE:NONE FRAMES:{data_frames} PREGAP:0 PGTYPE:MODE1 PGSUB:NONE POSTGAP:0\0"
        ),
        format!(
            "TRACK:2 TYPE:AUDIO SUBTYPE:NONE FRAMES:{audio_frames} PREGAP:2 PGTYPE:AUDIO PGSUB:NONE POSTGAP:0\0"
        ),
    ] {
        w.add_metadata(KnownMetadata::CdRomTrack2, meta.as_bytes(), true)
            .unwrap();
    }
    w.finish().unwrap();
    let mut audio = vec![0u8; 2 * 2352];
    audio.extend_from_slice(&audio_bin);
    (data_bin, audio)
}

/// A disc in a CHD: an xdelta (or a PPF) patches its data track, the
/// audio track and its pregap come through, and the result is a new CHD.
#[test]
fn a_disc_patch_makes_a_new_chd() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let chd = incoming.join("Game.chd");
    let (data, audio) = write_cd(&chd);
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
    let dat = tmp.path().join("ps1.dat");
    std::fs::write(
        &dat,
        format!(
            "<?xml version=\"1.0\"?><datafile><header><name>Sony - PlayStation</name></header>\
             <game name=\"Game (Japan)\">{}{}</game></datafile>",
            rom("Game (Japan) (Track 1).bin", &data),
            rom("Game (Japan) (Track 2).bin", &audio)
        ),
    )
    .unwrap();
    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let base = row_id(&lib, "Game (Japan)");

    let mut english = data.clone();
    english[100..107].copy_from_slice(b"ENGLISH");
    let mut delta = Vec::new();
    oxidelta::engine::encode(&data, &english, &mut delta).unwrap();
    let patch = tmp.path().join("Game English.xdelta");
    std::fs::write(&patch, &delta).unwrap();
    let plan = lib
        .patch_plan(&base, &patch, Some(Relation::Translation), Some("English"))
        .unwrap();
    assert!(plan.disc);
    let done = lib.patch_apply(&plan).unwrap();
    let target = home
        .games()
        .join("PlayStation/Game (Japan) [Translation- English].chd");
    assert_eq!(done.target, target);
    let tracks = romforge_core::disc::chd_members(&target).unwrap();
    assert_eq!(tracks.len(), 2, "{tracks:?}");
    assert_eq!(tracks[0].1.raw.sha1, sha1(&english), "the data track patched");
    assert_eq!(
        tracks[1].1.raw.sha1,
        sha1(&audio),
        "the audio track and its pregap kept"
    );
    assert_eq!(done.sha1, sha1(&english));
    let before = romforge_core::disc::chd_members(&chd).unwrap();
    assert_eq!(before[0].1.raw.sha1, sha1(&data), "the original untouched");
    let rows = lib.library_view().unwrap().rows;
    let copy = rows.iter().find(|r| r.title == plan.title).expect("listed");
    assert_eq!(copy.status, RowStatus::Derivative);

    // A PPF on the same disc, as a hack.
    let mut ppf = b"PPF30".to_vec();
    ppf.push(2);
    ppf.extend_from_slice(b"Hard Mode");
    ppf.extend_from_slice(&[b' '; 41]);
    ppf.extend_from_slice(&[0, 0, 0, 0]);
    ppf.extend_from_slice(&200u64.to_le_bytes());
    ppf.push(4);
    ppf.extend_from_slice(b"HARD");
    let ppf_path = tmp.path().join("hard.ppf");
    std::fs::write(&ppf_path, &ppf).unwrap();
    let plan = lib
        .patch_plan(&base, &ppf_path, Some(Relation::Hack), None)
        .unwrap();
    assert_eq!(
        plan.title, "Game (Japan) [Hack: Hard Mode]",
        "the PPF's own description"
    );
    let done = lib.patch_apply(&plan).unwrap();
    let tracks = romforge_core::disc::chd_members(&done.target).unwrap();
    let mut hard = data.clone();
    hard[200..204].copy_from_slice(b"HARD");
    assert_eq!(tracks[0].1.raw.sha1, sha1(&hard));

    // An xdelta made from the second track: the first refuses it (its
    // checksums don't match), so it goes on the second.
    let mut remix = audio.clone();
    remix[5000..5005].copy_from_slice(b"REMIX");
    let mut delta = Vec::new();
    oxidelta::engine::encode(&audio, &remix, &mut delta).unwrap();
    let patch = tmp.path().join("remix.xdelta");
    std::fs::write(&patch, &delta).unwrap();
    let plan = lib.patch_plan(&base, &patch, Some(Relation::Hack), None).unwrap();
    let done = lib.patch_apply(&plan).unwrap();
    let tracks = romforge_core::disc::chd_members(&done.target).unwrap();
    assert_eq!(tracks[0].1.raw.sha1, sha1(&data), "the first track as it was");
    assert_eq!(tracks[1].1.raw.sha1, sha1(&remix), "the second track patched");
    assert_eq!(done.sha1, sha1(&remix));
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

/// Patches a scan finds join their game: a BPS by the ROM it was made
/// from, an IPS beside the ROM of the same name, a zipped patch by its
/// readme's title. A ROM no DAT knows that a paired BPS beside it makes
/// from the verified game is named a translation of it, with no click,
/// never verified; taken off by the owner, it isn't named again.
#[test]
fn scanned_patches_pair_with_their_game() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    std::fs::copy(fixtures().join("roms/gb/Test GB.gb"), incoming.join("Test GB.gb")).unwrap();
    let data = std::fs::read(incoming.join("Test GB.gb")).unwrap();
    let mut english = data.clone();
    english[0x134..0x138].copy_from_slice(b"ENGL");
    // A BPS with a name that says nothing of the game, and the ROM it makes.
    std::fs::write(
        incoming.join("mtl_v1.bps"),
        bps(data.len() as u64, crc32fast::hash(&data), &english),
    )
    .unwrap();
    std::fs::write(incoming.join("Some download.gb"), &english).unwrap();
    // An IPS of the ROM's own name; a zip with a readme naming the game.
    std::fs::write(incoming.join("Test GB.ips"), ips(4, b"HARD")).unwrap();
    let mut z = zip::ZipWriter::new(std::fs::File::create(incoming.join("xyz123.zip")).unwrap());
    z.start_file("patch.ips", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(&ips(8, b"EASY")).unwrap();
    z.start_file("readme.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(b"=====\n  Test GB  \n=====\nAn easy-mode hack.\n")
        .unwrap();
    z.finish().unwrap();
    // Not for this game.
    std::fs::write(incoming.join("Other Game.ips"), ips(0, b"X")).unwrap();

    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    let report = lib.scan(&incoming, Scope::All).unwrap();
    assert!(
        report.skipped_kinds.iter().any(|k| k.ext == "ips"),
        "patches aren't hashed as ROMs"
    );
    let base = row_id(&lib, "Test GB");
    let d = lib.game_detail(&base).unwrap();
    let by: Vec<(String, String)> = d
        .patches
        .iter()
        .map(|p| (p.name.clone(), p.paired_by.clone()))
        .collect();
    assert!(by.contains(&("mtl_v1.bps".into(), "source".into())), "{by:?}");
    assert!(by.contains(&("Test GB.ips".into(), "beside".into())), "{by:?}");
    assert!(by.contains(&("patch.ips".into(), "title".into())), "{by:?}");
    assert_eq!(d.patches.len(), 3, "{by:?}");
    let offered = lib.patches_beside(&base).unwrap();
    let other = offered
        .iter()
        .find(|p| p.name == "Other Game.ips")
        .expect("in the folder");
    assert_eq!(other.paired_by, "folder");
    let easy = d.patches.iter().find(|p| p.name == "patch.ips").unwrap();
    assert!(easy.path.ends_with("xyz123.zip"), "the zip is what's applied");
    assert_eq!(easy.relation, Relation::Hack);
    // Planned without saying: the guess from its names.
    let plan = lib.patch_plan(&base, &easy.path, None, None).unwrap();
    assert_eq!(plan.relation, Relation::Hack);

    // The download is the BPS applied to the verified game: named so.
    let rows = lib.library_view().unwrap().rows;
    let named = rows
        .iter()
        .find(|r| r.title == "Test GB [Hack: mtl v1]")
        .unwrap_or_else(|| panic!("{rows:#?}"));
    assert_eq!(named.status, RowStatus::Derivative, "never verified");
    let nd = lib.game_detail(&named.id).unwrap();
    assert_eq!(nd.proof.as_deref(), Some("mtl_v1.bps"));
    assert_eq!(nd.derived.as_ref().map(|x| x.base.as_str()), Some("Test GB"));

    // Taken off: it stays unknown, rescans too.
    lib.forget_identity(&named.id).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let rows = lib.library_view().unwrap().rows;
    assert!(
        !rows.iter().any(|r| r.title == "Test GB [Hack: mtl v1]"),
        "{rows:#?}"
    );

    // A patch that goes away is forgotten.
    std::fs::remove_file(incoming.join("Test GB.ips")).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let d = lib.game_detail(&row_id(&lib, "Test GB")).unwrap();
    assert_eq!(d.patches.len(), 2);
}

/// The list puts each game's original on top and its versions under
/// headings: other regions, editions, translations, hacks, and patch files
/// apart. A game you have only a patch of is still listed, as missing,
/// with the patch under it.
#[test]
fn the_list_shows_originals_with_their_versions() {
    use romforge_core::view::Category;
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let incoming = tmp.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let games = [
        ("Quest (USA)", pattern(1, 2048)),
        ("Quest (Japan)", pattern(2, 2048)),
        ("Quest (USA) (Limited Edition)", pattern(3, 2048)),
        ("Other Story (USA)", pattern(4, 2048)),
    ];
    let mut dat =
        String::from("<?xml version=\"1.0\"?><datafile><header><name>Nintendo - Game Boy</name></header>");
    for (name, d) in &games {
        let h = romforge_core::hash::hash_stream(&d[..], d.len() as u64)
            .unwrap()
            .raw;
        dat += &format!(
            "<game name=\"{name}\"><rom name=\"{name}.gb\" size=\"{}\" crc=\"{}\" md5=\"{}\" sha1=\"{}\"/></game>",
            d.len(),
            h.crc32,
            h.md5,
            h.sha1
        );
    }
    dat += "</datafile>";
    std::fs::write(tmp.path().join("gb.dat"), dat).unwrap();
    for (name, d) in &games[..3] {
        std::fs::write(incoming.join(format!("{name}.gb")), d).unwrap();
    }
    // A translation of the Japanese release, and the copy it made.
    let japan = &games[1].1;
    let mut english = japan.clone();
    english[0..4].copy_from_slice(b"ENGL");
    std::fs::write(
        incoming.join("English.bps"),
        bps(japan.len() as u64, crc32fast::hash(japan), &english),
    )
    .unwrap();
    std::fs::write(incoming.join("Quest translated.gb"), &english).unwrap();
    // A hack patch of the US release, beside it.
    std::fs::write(incoming.join("Quest (USA).ips"), ips(4, b"HARD")).unwrap();
    // A translation of a game you don't have, named by its readme.
    let mut z = zip::ZipWriter::new(std::fs::File::create(incoming.join("os-en.zip")).unwrap());
    z.start_file("os.ips", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(&ips(0, b"EN")).unwrap();
    z.start_file("readme.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(b"v1.0\n\nAn English translation of Other Story for the Game Boy.\n")
        .unwrap();
    z.finish().unwrap();

    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(tmp.path().join("gb.dat"), None).unwrap();
    lib.scan(&incoming, Scope::All).unwrap();
    let rows = lib.library_view().unwrap().rows;
    let find = |t: &str| {
        rows.iter()
            .find(|r| r.title == t)
            .unwrap_or_else(|| panic!("no {t}: {rows:#?}"))
    };
    let usa = find("Quest (USA)");
    assert!(usa.main && usa.status == RowStatus::Verified);
    assert_eq!(usa.versions, 6, "{rows:#?}");
    let of = |t: &str| {
        let r = find(t);
        assert_eq!(r.group, usa.group, "{t}");
        assert!(!r.main, "{t}");
        r.category
    };
    assert_eq!(of("Quest (Japan)"), Some(Category::Region));
    assert_eq!(of("Quest (USA) (Limited Edition)"), Some(Category::Edition));
    assert_eq!(
        of("Quest (Japan) [Translation: English]"),
        Some(Category::Translation)
    );
    assert_eq!(of("English.bps"), Some(Category::PatchTranslation));
    assert_eq!(of("Quest (USA).ips"), Some(Category::PatchHack));
    assert_eq!(find("Quest (USA).ips").base_id.as_ref(), Some(&usa.id));
    assert_eq!(
        find("English.bps").base_id.as_ref(),
        Some(&find("Quest (Japan)").id),
        "for the release it was made from"
    );

    let other = find("Other Story (USA)");
    assert_eq!(other.status, RowStatus::Missing);
    assert!(other.main);
    let patch = find("os.ips");
    assert_eq!(patch.status, RowStatus::Patch);
    assert_eq!(patch.label.as_deref(), Some("os.ips (os-en.zip)"));
    assert_eq!(patch.group, other.group);
    assert_eq!(patch.category, Some(Category::PatchTranslation));
    assert_eq!(patch.base_id, None, "nothing to apply it to");
    let d = lib.game_detail(&patch.id).unwrap();
    assert_eq!(d.patches.len(), 1);
    let d = lib.game_detail(&other.id).unwrap();
    assert_eq!(d.patches.len(), 1, "the missing game lists its patch");
}
