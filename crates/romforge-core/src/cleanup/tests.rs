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
