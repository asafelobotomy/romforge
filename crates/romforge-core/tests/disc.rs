//! Milestone 4: CHD tracks hash like the Redump `.bin` files they came from,
//! and sheets are checked. The CHDs are written here with the `chd` crate's
//! encoder, so no external tool is needed.

use std::io::Write as _;
use std::path::Path;

use chd::metadata::KnownMetadata;
use chd::write::{ChdWriteOptions, ChdWriter};
use romforge_core::hash::hash_stream;
use romforge_core::{Library, Scope, Status};

fn pattern(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

/// A two-track CD: a MODE1_RAW data track and an audio track whose 2-frame
/// pregap isn't stored. Returns the CHD path and the `.bin` contents.
fn write_cd(dir: &Path) -> (std::path::PathBuf, Vec<u8>, Vec<u8>) {
    let data_frames = 10usize;
    let audio_frames = 7usize;
    let data_bin = pattern(1, data_frames * 2352);
    let audio_bin = pattern(2, audio_frames * 2352);

    let mut frames: Vec<u8> = Vec::new();
    let mut push_track = |bin: &[u8], audio: bool, n: usize| {
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
        // Pad to 4 frames.
        for _ in 0..(4 - n % 4) % 4 {
            frames.extend_from_slice(&[0u8; 2448]);
        }
    };
    push_track(&data_bin, false, data_frames);
    push_track(&audio_bin, true, audio_frames);

    let path = dir.join("Game.chd");
    let mut w = ChdWriter::new(std::fs::File::create(&path).unwrap(), ChdWriteOptions::cd()).unwrap();
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

    // The audio .bin has the 2 pregap frames as silence, as Redump's does.
    let mut audio_with_pregap = vec![0u8; 2 * 2352];
    audio_with_pregap.extend_from_slice(&audio_bin);
    (path, data_bin, audio_with_pregap)
}

#[test]
fn chd_tracks_match_their_bins() {
    let dir = tempfile::tempdir().unwrap();
    let roms = dir.path().join("roms");
    std::fs::create_dir(&roms).unwrap();
    let (chd, data_bin, audio_bin) = write_cd(&roms);

    let members = romforge_core::disc::chd_members(&chd).unwrap();
    let names: Vec<&str> = members.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["Game (Track 1).bin", "Game (Track 2).bin"]);
    let h = |d: &[u8]| hash_stream(d, d.len() as u64).unwrap().raw;
    assert_eq!(members[0].1.raw, h(&data_bin));
    assert_eq!(members[1].1.raw, h(&audio_bin), "byte order and pregap restored");

    // End to end: a Redump-style DAT with the bins and a cue matches the CHD.
    let rom = |name: &str, d: &[u8]| {
        let x = h(d);
        format!(
            r#"<rom name="{name}" size="{}" crc="{}" md5="{}" sha1="{}"/>"#,
            x.size, x.crc32, x.md5, x.sha1
        )
    };
    let dat = dir.path().join("psx.dat");
    std::fs::write(
        &dat,
        format!(
            r#"<datafile><header><name>Sony - PlayStation</name></header><game name="Game (USA)">{}{}{}</game></datafile>"#,
            rom("Game (USA) (Track 1).bin", &data_bin),
            rom("Game (USA) (Track 2).bin", &audio_bin),
            rom("Game (USA).cue", b"FILE ..."),
        ),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    let d = lib.import_dat(&dat, None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    let files = lib.files(&Default::default()).unwrap();
    assert!(files.iter().all(|f| f.status == Status::Matched), "{files:?}");
    let game = &lib.dat_games(d.dat_id).unwrap()[0];
    assert!(game.have && game.cue_differs, "a CHD has no cue sheet: {game:?}");
}

#[test]
fn dvd_chd_hashes_as_one_iso() {
    let dir = tempfile::tempdir().unwrap();
    let iso = pattern(7, 2048 * 5);
    let path = dir.path().join("Disc.chd");
    let opts = ChdWriteOptions::dvd().logical_bytes(iso.len() as u64);
    let mut w = ChdWriter::new(std::fs::File::create(&path).unwrap(), opts).unwrap();
    w.write_all(&iso).unwrap();
    w.finish().unwrap();
    let members = romforge_core::disc::chd_members(&path).unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].0, "Disc.iso");
    assert_eq!(
        members[0].1.raw,
        hash_stream(&iso[..], iso.len() as u64).unwrap().raw
    );
}

#[test]
fn scan_reports_cue_sheets_with_missing_tracks() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.bin"), [0u8; 2352]).unwrap();
    std::fs::write(
        dir.path().join("Game.cue"),
        "FILE \"a.bin\" BINARY\n  TRACK 01 MODE2/2352\nFILE \"b.bin\" BINARY\n  TRACK 02 AUDIO\n",
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    let report = lib.scan(dir.path(), Scope::All).unwrap();
    assert_eq!(report.sheet_problems.len(), 1, "{:?}", report.sheet_problems);
    assert!(report.sheet_problems[0].1.ends_with("b.bin"));
}
