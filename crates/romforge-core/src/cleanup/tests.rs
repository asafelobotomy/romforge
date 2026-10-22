#[test]
fn unsafe_member_names() {
    use super::items::safe_member_name as ok;
    assert!(ok("Game (USA).sfc") && ok("Disk 1/Game.adf"));
    for bad in [
        "../evil.sfc",
        "/etc/passwd",
        "a/../b",
        "C:\\x",
        "a\\b",
        "romforge/x.png",
        "",
        "a//b",
        "./a",
    ] {
        assert!(!ok(bad), "{bad}");
    }
}

#[test]
fn disk_numbers() {
    assert_eq!(
        disk_of("Ys (1987)(Nihon Falcom)(Disk 2 of 3)"),
        Some((2, Some(3), "Ys (1987)(Nihon Falcom)".into()))
    );
    assert_eq!(
        disk_of("Final Fantasy VII (USA) (Disc 1)"),
        Some((1, None, "Final Fantasy VII (USA)".into()))
    );
    assert_eq!(disk_of("Game (Disk A)"), None);
    assert_eq!(disk_of("Game (USA)"), None);
}

use super::chd::{cue_pregaps, track_type};
use super::parts::natural_key;
use super::*;

#[test]
fn natural_order() {
    let mut v = vec![
        "Disc (Track 10).bin",
        "Disc (Track 2).bin",
        "Disc (Track 1).bin",
        "Disc.cue",
    ];
    v.sort_by_key(|n| natural_key(n));
    assert_eq!(
        v,
        [
            "Disc (Track 1).bin",
            "Disc (Track 2).bin",
            "Disc (Track 10).bin",
            "Disc.cue"
        ]
    );
}

#[test]
fn cue_pregaps_by_track() {
    let cue = "FILE \"a (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\nFILE \"a (Track 2).bin\" BINARY\n  TRACK 02 AUDIO\n    INDEX 00 00:00:00\n    INDEX 01 00:02:00\n";
    assert_eq!(cue_pregaps(cue).unwrap(), [0, 150]);
    // Untrusted times: one that overflows is unreadable, not wrapped.
    let huge = format!(
        "  TRACK 01 AUDIO\n    INDEX 00 {}:00:00\n    INDEX 01 00:02:00\n",
        u64::MAX
    );
    assert_eq!(cue_pregaps(&huge), None);
    assert_eq!(cue_pregaps("  TRACK 01 AUDIO\n    INDEX 01 00:xx:00\n"), None);
}

#[test]
fn track_types_from_sectors() {
    let mut s = vec![0u8; 2352];
    assert_eq!(track_type(&s), "AUDIO");
    s[1..11].fill(255);
    s[15] = 2;
    assert_eq!(track_type(&s), "MODE2_RAW");
    s[15] = 1;
    assert_eq!(track_type(&s), "MODE1_RAW");
}

/// A big zip member streams from the file itself, stored or deflated.
#[test]
fn zip_members_stream_in_place() {
    use std::io::{Read, Write};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.zip");
    let data: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
    let mut z = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (name, method) in [
        ("stored.bin", zip::CompressionMethod::Stored),
        ("deflated.bin", zip::CompressionMethod::Deflated),
    ] {
        let o = zip::write::SimpleFileOptions::default().compression_method(method);
        z.start_file(name, o).unwrap();
        z.write_all(&data).unwrap();
    }
    z.finish().unwrap();
    let mut a = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    for name in ["stored.bin", "deflated.bin"] {
        let m = a.by_name(name).unwrap();
        let mut r = super::parts::stream_member(&path, &m).unwrap().expect(name);
        let mut back = Vec::new();
        r.read_to_end(&mut back).unwrap();
        assert!(back == data, "{name}");
    }
}
