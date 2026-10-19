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
//! Cue and GDI sheets.

use romforge_core::hash::hash_stream;
use romforge_core::{Library, LibraryFilter, Scope, Status};

mod common;
use common::fixture_data::*;

#[test]
fn disc_with_a_different_cue_sheet_counts_as_have() {
    use romforge_core::hash::hash_stream;
    let dir = tempfile::tempdir().unwrap();
    let track = vec![7u8; 4096];
    let cue = b"FILE \"Disc (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n".to_vec();
    let h = |d: &[u8]| hash_stream(d, d.len() as u64).unwrap().raw;
    let (t, c) = (h(&track), h(&cue));
    let rom = |name: &str, d: &romforge_core::hash::Digest| {
        format!(
            r#"<rom name="{name}" size="{}" crc="{}" md5="{}" sha1="{}"/>"#,
            d.size, d.crc32, d.md5, d.sha1
        )
    };
    let dat = format!(
        r#"<?xml version="1.0"?><datafile><header><name>Test - Discs</name></header>
<game name="Disc"><description>Disc</description>{}{}</game>
<game name="Sheets only">{}</game></datafile>"#,
        rom("Disc (Track 1).bin", &t),
        rom("Disc.cue", &c),
        rom("Only.cue", &c),
    );
    let dat_path = dir.path().join("discs.dat");
    std::fs::write(&dat_path, dat).unwrap();
    let roms = dir.path().join("roms");
    std::fs::create_dir(&roms).unwrap();
    std::fs::write(roms.join("Disc (Track 1).bin"), &track).unwrap();
    std::fs::write(roms.join("Disc.cue"), b"FILE \"disc.bin\" BINARY\r\n").unwrap();

    let mut lib = Library::open_in_memory().unwrap();
    let d = lib.import_dat(&dat_path, Some("PlayStation")).unwrap();
    lib.scan(&roms, Scope::Dat(d.dat_id)).unwrap();
    let games = lib.dat_games(d.dat_id).unwrap();
    let disc = games.iter().find(|g| g.name == "Disc").unwrap();
    assert!(disc.have && disc.cue_differs, "{disc:?}");
    assert_eq!((disc.roms_have, disc.roms_total), (1, 2));
    let only = games.iter().find(|g| g.name == "Sheets only").unwrap();
    assert!(!only.have, "a game of only sheets needs them to match");
    let s = lib.summary().unwrap();
    assert_eq!((s.dats[0].have, s.dats[0].missing), (1, 1));

    // The exact cue sheet makes it a plain have.
    std::fs::write(roms.join("Disc.cue"), &cue).unwrap();
    lib.scan(&roms, Scope::Dat(d.dat_id)).unwrap();
    let games = lib.dat_games(d.dat_id).unwrap();
    let disc = games.iter().find(|g| g.name == "Disc").unwrap();
    assert!(disc.have && !disc.cue_differs);
    let only = games.iter().find(|g| g.name == "Sheets only").unwrap();
    assert!(only.have, "the same sheet satisfies the sheet-only game");
}

/// A cue sheet rewritten by some tool (CRLF here) matches no hash, but when
/// its FILE lines name exactly the game's tracks, all matched beside it,
/// it is a `sheet` with the game, not an unknown file. In a folder and in a
/// zip; one naming other files stays unknown.
#[test]
fn rewritten_cue_sheets_fit_their_tracks() {
    use std::io::Write as _;
    let dir = tempfile::tempdir().unwrap();
    let t1 = body("cue-t1", 2352 * 4);
    let t2 = body("cue-t2", 2352 * 3);
    let redump = "FILE \"Game (USA) (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\nFILE \"Game (USA) (Track 2).bin\" BINARY\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n";
    let rewritten = redump.replace('\n', "\r\n");
    let wrong = rewritten.replace("(Track 2)", "(Track 9)");
    let h = |d: &[u8]| hash_stream(d, d.len() as u64).unwrap().raw;
    let rom = |name: &str, d: &[u8]| {
        let x = h(d);
        format!(
            r#"<rom name="{name}" size="{}" crc="{}" md5="{}" sha1="{}"/>"#,
            x.size, x.crc32, x.md5, x.sha1
        )
    };
    std::fs::write(
        dir.path().join("psx.dat"),
        format!(
            r#"<datafile><header><name>Sony - PlayStation</name></header><game name="Game (USA)">{}{}{}</game></datafile>"#,
            rom("Game (USA).cue", redump.as_bytes()),
            rom("Game (USA) (Track 1).bin", &t1),
            rom("Game (USA) (Track 2).bin", &t2),
        ),
    )
    .unwrap();
    let roms = dir.path().join("roms");
    // A GDI sheet someone made for the same tracks fits as well.
    let gdi = "2\n1 0 4 2352 \"Game (USA) (Track 1).bin\" 0\n2 600 0 2352 \"Game (USA) (Track 2).bin\" 0\n"
        .to_owned();
    let gdi_dir = roms.join("gdi");
    std::fs::create_dir_all(&gdi_dir).unwrap();
    std::fs::write(gdi_dir.join("Game (USA) (Track 1).bin"), &t1).unwrap();
    std::fs::write(gdi_dir.join("Game (USA) (Track 2).bin"), &t2).unwrap();
    std::fs::write(gdi_dir.join("Game (USA).gdi"), &gdi).unwrap();
    for (sub, cue) in [("folder", &rewritten), ("other", &wrong)] {
        let d = roms.join(sub);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("Game (USA) (Track 1).bin"), &t1).unwrap();
        std::fs::write(d.join("Game (USA) (Track 2).bin"), &t2).unwrap();
        std::fs::write(d.join("Game (USA).cue"), cue).unwrap();
    }
    {
        let mut z = zip::ZipWriter::new(std::fs::File::create(roms.join("Game (USA).zip")).unwrap());
        for (n, d) in [
            ("Game (USA) (Track 1).bin", &t1[..]),
            ("Game (USA) (Track 2).bin", &t2[..]),
            ("Game (USA).cue", rewritten.as_bytes()),
        ] {
            z.start_file(n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(d).unwrap();
        }
        z.finish().unwrap();
    }
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(dir.path().join("psx.dat"), None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    let files = lib.files(&LibraryFilter::default()).unwrap();
    let status_of = |needle: &str| {
        files
            .iter()
            .filter(|f| f.display_path().contains(needle) && f.display_path().ends_with(".cue"))
            .map(|f| f.status)
            .collect::<Vec<_>>()
    };
    assert_eq!(status_of("/folder/"), [Status::Sheet]);
    assert_eq!(status_of(".zip#"), [Status::Sheet]);
    assert_eq!(
        status_of("/other/"),
        [Status::Unknown],
        "it names a track the game lacks"
    );
    let gdi_status: Vec<_> = files
        .iter()
        .filter(|f| f.display_path().ends_with("/gdi/Game (USA).gdi"))
        .map(|f| f.status)
        .collect();
    assert_eq!(gdi_status, [Status::Sheet]);

    let v = lib.library_view().unwrap();
    let rows: Vec<_> = v.rows.iter().filter(|r| r.title == "Game (USA)").collect();
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|r| r.cue_differs && r.cue_fits), "{rows:?}");
    // The fitting cue is one of the game's files, not a row of its own.
    assert!(
        !v.rows
            .iter()
            .any(|r| r.title.ends_with("(USA).cue") && r.id.starts_with("f:") && {
                let d = lib.game_detail(&r.id).unwrap();
                d.files.iter().any(|f| !f.path.contains("/other/"))
            })
    );
}

/// A sheet fits only tracks in its own folder: `a_b` is not `aXb` (`_` and
/// `%` in folder names are plain characters).
#[test]
fn a_sheet_fits_only_its_own_folder() {
    let dir = tempfile::tempdir().unwrap();
    let t1 = body("wild-t1", 2352 * 4);
    let cue = "FILE \"Game (USA).bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\n";
    let dat_cue = cue.replace("\r\n", "\n");
    let h = |d: &[u8]| hash_stream(d, d.len() as u64).unwrap().raw;
    let rom = |name: &str, d: &[u8]| {
        let x = h(d);
        format!(
            r#"<rom name="{name}" size="{}" crc="{}" md5="{}" sha1="{}"/>"#,
            x.size, x.crc32, x.md5, x.sha1
        )
    };
    std::fs::write(
        dir.path().join("psx.dat"),
        format!(
            r#"<datafile><header><name>Sony - PlayStation</name></header><game name="Game (USA)">{}{}</game></datafile>"#,
            rom("Game (USA).cue", dat_cue.as_bytes()),
            rom("Game (USA).bin", &t1),
        ),
    )
    .unwrap();
    let roms = dir.path().join("roms");
    std::fs::create_dir_all(roms.join("a_b")).unwrap();
    std::fs::create_dir_all(roms.join("aXb")).unwrap();
    std::fs::write(roms.join("a_b").join("Game (USA).cue"), cue).unwrap();
    std::fs::write(roms.join("aXb").join("Game (USA).bin"), &t1).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(dir.path().join("psx.dat"), None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    let files = lib.files(&LibraryFilter::default()).unwrap();
    let cue_status: Vec<_> = files
        .iter()
        .filter(|f| f.display_path().ends_with("/a_b/Game (USA).cue"))
        .map(|f| f.status)
        .collect();
    assert_eq!(cue_status, [Status::Unknown], "its track is in another folder");
}
