//! The README quick start, run against the real binary.

use std::path::Path;
use std::process::Command;

fn run(db: &Path, args: &[&str]) -> String {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let out = Command::new(env!("CARGO_BIN_EXE_romforge-cli"))
        .current_dir(&fixtures)
        .env("ROMFORGE_DB", db)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn import_scan_report() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("romforge.db");
    let mut import = vec!["dat", "import"];
    let dats: Vec<String> = (["a7800", "fds", "gb", "gba", "lynx", "md", "nes", "snes"])
        .iter()
        .map(|d| format!("dats/{d}.dat"))
        .collect();
    import.extend(dats.iter().map(String::as_str));
    let out = run(&db, &import);
    assert!(
        out.contains("[NES] 4 games, 4 roms, header No-Intro_NES.xml"),
        "{out}"
    );

    let out = run(&db, &["scan", "."]);
    assert!(
        out.contains("matched       roms/nes/Test NES (iNES).nes [ines]  Test NES (iNES)"),
        "{out}"
    );
    assert!(
        out.contains("candidate     roms/gb/CRC Only.gb  CRC Only"),
        "{out}"
    );
    assert!(
        out.contains("14 matched, 1 candidate, 1 unknown, 0 wrong_system, 1 duplicate"),
        "{out}"
    );

    assert!(
        out.contains(
            "; skipped 14 non-ROM files (.dat 8, .xdelta 2, .mkv 1, .nfo 1, .txt 1, .url 1); skipped 1 symlinked folder"
        ),
        "{out}"
    );
    assert!(!out.contains("CDRomance.url"), "{out}");

    let out = run(&db, &["scan", "roms", "--all-files"]);
    assert!(
        out.contains("unknown       archives/Test GBA.zip#CDRomance.url"),
        "{out}"
    );
    let out = run(&db, &["scan", "."]);
    assert!(out.contains("14 matched"), "{out}");

    let out = run(&db, &["report"]);
    assert!(out.contains("files: 14 matched"), "{out}");

    let out = run(&db, &["scan", "roms/gb", "--system", "nes", "-q"]);
    assert!(out.contains("3 wrong_system"), "{out}");
}
