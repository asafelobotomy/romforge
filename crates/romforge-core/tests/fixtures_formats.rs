//! End-to-end tests over the committed fixtures in `fixtures/`.
//!
//! The fixtures are tiny synthetic files, not real ROMs. They are written by
//! the ignored `regenerate_fixtures` test:
//!
//!     cargo test -p romforge-core --test fixtures -- --ignored regenerate
//!
//! DAT hashes are computed over the ROM *body* directly, never through the
//! header detector, so the tests check the detector rather than mirror it.
//!
//! Byte orders, header IDs, archives and edge cases.

use romforge_core::hash::hash_stream;
use romforge_core::{Library, LibraryFilter, Scope, Status};

mod common;
use common::fixture_data::*;
use common::fixture_lib::*;
use common::fixtures;

/// N64 dumps in `.v64` / `.n64` byte order and `.smd` Mega Drive dumps
/// match the DAT's `.z64` / `.md` hashes (in temp files; the committed
/// fixtures stay as they are).
#[test]
fn other_byte_orders_match_the_dat() {
    let dir = tempfile::tempdir().unwrap();
    let mut z64 = body("z64", 32 * 1024);
    z64[..4].copy_from_slice(&[0x80, 0x37, 0x12, 0x40]);
    let md = body("md-smd", 32 * 1024);
    let v64: Vec<u8> = z64.chunks(2).flat_map(|w| [w[1], w[0]]).collect();
    let n64: Vec<u8> = z64.chunks(4).flat_map(|w| [w[3], w[2], w[1], w[0]]).collect();
    let mut smd = header(8, &[0xAA, 0xBB], 512);
    smd[1] = 3;
    for block in md.chunks(16 * 1024) {
        smd.extend(block.iter().skip(1).step_by(2));
        smd.extend(block.iter().step_by(2));
    }
    let roms = dir.path().join("roms");
    std::fs::create_dir_all(&roms).unwrap();
    std::fs::write(roms.join("Game (v64).v64"), &v64).unwrap();
    std::fs::write(roms.join("Game (n64).n64"), &n64).unwrap();
    std::fs::write(roms.join("Game.smd"), &smd).unwrap();
    let dat = |name: &str, game: &str, rom: &str, data: &[u8]| {
        let h = hash_stream(data, data.len() as u64).unwrap().raw;
        format!(
            r#"<datafile><header><name>{name}</name></header><game name="{game}"><rom name="{rom}" size="{}" crc="{}" md5="{}" sha1="{}"/></game></datafile>"#,
            h.size, h.crc32, h.md5, h.sha1
        )
    };
    std::fs::write(
        dir.path().join("n64.dat"),
        dat("Nintendo - Nintendo 64", "Game (USA)", "Game (USA).z64", &z64),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("md.dat"),
        dat(
            "Sega - Mega Drive - Genesis",
            "Game (Japan)",
            "Game (Japan).md",
            &md,
        ),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(dir.path().join("n64.dat"), None).unwrap();
    lib.import_dat(dir.path().join("md.dat"), None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    let files = lib.files(&LibraryFilter::default()).unwrap();
    assert_eq!(files.len(), 3);
    let mut matched = 0;
    for f in &files {
        match f.status {
            Status::Matched => matched += 1,
            // Both N64 files are the same game: one counts, one is a copy.
            Status::Duplicate => assert!(f.display_path().ends_with("64")),
            s => panic!("{}: {s:?}", f.display_path()),
        }
    }
    assert_eq!(matched, 2, "the N64 game and the Mega Drive one");
}

/// A file's own header is read while it is hashed, after a copier header
/// is stripped, and shown in Details: for files no DAT knows, too.
#[test]
fn game_ids_from_headers_and_boot_files() {
    let dir = tempfile::tempdir().unwrap();
    let roms = dir.path().join("roms");
    std::fs::create_dir_all(&roms).unwrap();
    // A LoROM SNES game with a 512-byte copier header in front.
    let mut snes = vec![0u8; 512];
    let mut body = vec![0xFFu8; 0x8000];
    let h = 0x7FC0;
    body[h..h + 21].copy_from_slice(b"DRAGONQUEST3         ");
    body[h + 0x19] = 0; // Japan
    body[h + 0x1C..h + 0x20].copy_from_slice(&[0x1A, 0x93, 0xE5, 0x6C]);
    snes.extend_from_slice(&body);
    std::fs::write(roms.join("dq3 translated.smc"), &snes).unwrap();
    // A PS1 data track: raw Mode 2 sectors, SYSTEM.CNF in the root.
    let cnf = b"BOOT = cdrom:\\SLUS_007.07;1\r\n";
    let mut user = vec![[0u8; 2048]; 20];
    user[16][0] = 1;
    user[16][1..6].copy_from_slice(b"CD001");
    user[16][156] = 34;
    user[16][158..162].copy_from_slice(&18u32.to_le_bytes());
    user[16][166..170].copy_from_slice(&2048u32.to_le_bytes());
    let name = b"SYSTEM.CNF;1";
    user[18][0] = (33 + name.len()) as u8;
    user[18][2..6].copy_from_slice(&19u32.to_le_bytes());
    user[18][10..14].copy_from_slice(&(cnf.len() as u32).to_le_bytes());
    user[18][32] = name.len() as u8;
    user[18][33..33 + name.len()].copy_from_slice(name);
    user[19][..cnf.len()].copy_from_slice(cnf);
    let mut track = Vec::new();
    for s in &user {
        let mut sector = [0u8; 2352];
        sector[..12].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0]);
        sector[15] = 2;
        sector[24..2072].copy_from_slice(s);
        track.extend_from_slice(&sector);
    }
    std::fs::write(roms.join("silent hill.bin"), &track).unwrap();

    let mut lib = Library::open_in_memory().unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    let view = lib.library_view().unwrap();
    let id_of = |needle: &str| {
        let row = view.rows.iter().find(|r| r.title.contains(needle)).unwrap();
        lib.game_detail(&row.id).unwrap().files[0]
            .game_id
            .clone()
            .unwrap()
    };
    let snes = id_of("dq3");
    assert_eq!((snes.kind, snes.title.as_deref()), ("snes", Some("DRAGONQUEST3")));
    assert_eq!(snes.region.as_deref(), Some("Japan"));
    let ps1 = id_of("silent");
    assert_eq!((ps1.kind, ps1.serial.as_deref()), ("ps1", Some("SLUS_007.07")));
}

/// In a solid 7z every member is one stream: a ROM after a skipped
/// (non-ROM) member must still hash as itself.
#[test]
fn solid_7z_members_after_a_skipped_one_hash_right() {
    let dir = tempfile::tempdir().unwrap();
    let roms = dir.path().join("roms");
    std::fs::create_dir_all(&roms).unwrap();
    let gba = body("solid-gba", 4096);
    let notes = body("solid-notes", 3000);
    {
        let mut sz = sevenz_rust2::ArchiveWriter::create(roms.join("s.7z")).unwrap();
        sz.push_archive_entries(
            vec![
                sevenz_rust2::ArchiveEntry::new_file("a notes.txt"),
                sevenz_rust2::ArchiveEntry::new_file("z game.gba"),
            ],
            vec![
                sevenz_rust2::SourceReader::new(&notes[..]),
                sevenz_rust2::SourceReader::new(&gba[..]),
            ],
        )
        .unwrap();
        sz.finish().unwrap();
    }
    let h = hash_stream(&gba[..], gba.len() as u64).unwrap().raw;
    std::fs::write(
        dir.path().join("gba.dat"),
        format!(
            r#"<datafile><header><name>Nintendo - Game Boy Advance</name></header><game name="Solid (USA)"><rom name="Solid (USA).gba" size="{}" crc="{}" md5="{}" sha1="{}"/></game></datafile>"#,
            h.size, h.crc32, h.md5, h.sha1
        ),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(dir.path().join("gba.dat"), None).unwrap();
    let report = lib.scan(&roms, Scope::All).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let files = lib.files(&LibraryFilter::default()).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].status, Status::Matched);
}

/// A CRC32 alone never names a file (house rule 8): an unknown file whose
/// CRC32 and size happen to equal another system's ROM keeps its own name.
#[test]
fn crc_only_hits_never_name_unknown_files() {
    let dir = tempfile::tempdir().unwrap();
    let roms = dir.path().join("roms");
    std::fs::create_dir_all(&roms).unwrap();
    let data = body("crc-twin", 1024);
    std::fs::write(roms.join("my file.gb"), &data).unwrap();
    let h = hash_stream(&data[..], data.len() as u64).unwrap().raw;
    // Another system's DAT lists a ROM with only that size and CRC32.
    std::fs::write(
        dir.path().join("nes.dat"),
        format!(
            r#"<datafile><header><name>Nintendo - Nintendo Entertainment System</name></header><game name="Twin (USA)"><rom name="Twin (USA).nes" size="{}" crc="{}"/></game></datafile>"#,
            h.size, h.crc32
        ),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(dir.path().join("nes.dat"), None).unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    let gb = dat_id(&lib, "Nintendo - Game Boy");
    lib.scan(&roms, Scope::Dat(gb)).unwrap();
    let files = lib.files(&LibraryFilter::default()).unwrap();
    assert_eq!(files[0].status, Status::Unknown);
    assert!(files[0].games.is_empty(), "{:?}", files[0].games);
    let v = lib.library_view().unwrap();
    assert_eq!(v.rows.len(), 1);
    assert_eq!(v.rows[0].title, "my file.gb", "not named after the NES game");
    assert_eq!(v.rows[0].system, None);
}

/// The library view is kept between calls, but a write through another
/// connection (the app reads on one and writes on another) is seen.
#[test]
fn the_cached_view_sees_other_connections() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("lib.db");
    let mut writer = Library::open(&db).unwrap();
    let reader = Library::open(&db).unwrap();
    writer.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    assert!(reader.library_view().unwrap().rows.is_empty());
    assert!(reader.library_view().unwrap().rows.is_empty(), "cached");
    writer.scan(fixtures().join("roms/gb"), Scope::All).unwrap();
    let rows = reader.library_view().unwrap().rows;
    assert!(rows.iter().any(|r| r.title == "Test GB"), "{rows:?}");
}

/// The copy that holds the whole game owns it, wherever it sorts; a
/// partial copy elsewhere is the duplicate. An empty file matches nothing.
#[test]
fn the_complete_copy_wins_and_empty_files_match_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let a = body("whole-a", 1000);
    let b = body("whole-b", 1200);
    let h = |d: &[u8]| hash_stream(d, d.len() as u64).unwrap().raw;
    let rom = |name: &str, d: &[u8]| {
        let x = h(d);
        format!(
            r#"<rom name="{name}" size="{}" crc="{}" md5="{}" sha1="{}"/>"#,
            x.size, x.crc32, x.md5, x.sha1
        )
    };
    std::fs::write(
        dir.path().join("t.dat"),
        format!(
            r#"<datafile><header><name>Nintendo - Game Boy</name></header>
               <game name="Two (USA)">{}{}</game><game name="Blank (USA)">{}</game></datafile>"#,
            rom("a.gb", &a),
            rom("b.gb", &b),
            rom("blank.gb", b""),
        ),
    )
    .unwrap();
    let roms = dir.path().join("roms");
    for (sub, parts) in [
        ("aaa", vec![("a.gb", &a)]),
        ("zzz", vec![("a.gb", &a), ("b.gb", &b)]),
    ] {
        std::fs::create_dir_all(roms.join(sub)).unwrap();
        for (n, d) in parts {
            std::fs::write(roms.join(sub).join(n), d).unwrap();
        }
    }
    std::fs::write(roms.join("blank.gb"), b"").unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(dir.path().join("t.dat"), None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    let files = lib.files(&LibraryFilter::default()).unwrap();
    let status_of = |end: &str| {
        files
            .iter()
            .find(|f| f.display_path().ends_with(end))
            .map(|f| f.status)
            .unwrap()
    };
    assert_eq!(status_of("/aaa/a.gb"), Status::Duplicate, "only half the game");
    assert_eq!(status_of("/zzz/a.gb"), Status::Matched);
    assert_eq!(status_of("/zzz/b.gb"), Status::Matched);
    assert_eq!(status_of("/blank.gb"), Status::Unknown);
}

/// A complete game from a DAT of no known system is one verified row.
#[test]
fn a_game_from_an_unknown_system_is_one_row() {
    let dir = tempfile::tempdir().unwrap();
    let a = body("nosys-a", 900);
    let b = body("nosys-b", 700);
    let h = |d: &[u8]| hash_stream(d, d.len() as u64).unwrap().raw;
    let rom = |name: &str, d: &[u8]| {
        let x = h(d);
        format!(
            r#"<rom name="{name}" size="{}" crc="{}" md5="{}" sha1="{}"/>"#,
            x.size, x.crc32, x.md5, x.sha1
        )
    };
    std::fs::write(
        dir.path().join("t.dat"),
        format!(
            r#"<datafile><header><name>Acme - Gadget 9000</name></header><game name="Pair (World)">{}{}</game></datafile>"#,
            rom("a.bin", &a),
            rom("b.bin", &b),
        ),
    )
    .unwrap();
    let roms = dir.path().join("roms");
    std::fs::create_dir_all(&roms).unwrap();
    std::fs::write(roms.join("a.bin"), &a).unwrap();
    std::fs::write(roms.join("b.bin"), &b).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(dir.path().join("t.dat"), None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    let rows = lib.library_view().unwrap().rows;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].title, "Pair (World)");
    assert_eq!(rows[0].status, romforge_core::view::RowStatus::Verified);
    assert_eq!(rows[0].system, None);
    let d = lib.game_detail(&rows[0].id).unwrap();
    assert_eq!(d.row.files, 2);
}
