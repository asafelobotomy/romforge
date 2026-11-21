//! `scan` reads ScummVM's list first, as the app's Scan folder does, so a
//! ScummVM game is one game rather than a row per data file. A stand-in
//! `scummvm` on `PATH` answers.

use std::path::Path;
use std::process::Command;

fn md5_hex(data: &[u8]) -> String {
    romforge_core::hash::hash_stream(data, data.len() as u64)
        .unwrap()
        .raw
        .md5
}

fn cli(db: &Path, path: &str, args: &[&str]) -> (String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_romforge-cli"))
        .env("ROMFORGE_DB", db)
        .env("PATH", path)
        .env("RUST_BACKTRACE", "0")
        .args(args)
        .output()
        .unwrap();
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    );
    assert!(out.status.success(), "{stderr}");
    (stdout, stderr)
}

#[test]
fn scan_names_a_scummvm_game_as_one() {
    let tmp = tempfile::tempdir().unwrap();
    let game = tmp.path().join("roms/Steel Sky (CD)");
    std::fs::create_dir_all(&game).unwrap();
    let dnr: Vec<u8> = (0..80u8).collect();
    let dsk: Vec<u8> = (0..9000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(game.join("SKY.DNR"), &dnr).unwrap();
    std::fs::write(game.join("SKY.DSK"), &dsk).unwrap();
    std::fs::write(game.join("SETUP.EXE"), b"MZ").unwrap();
    let list = tmp.path().join("list.dat");
    std::fs::write(
        &list,
        format!(
            "game (\n\tname \"sky\"\n\ttitle \"Beneath a Steel Sky\"\n\textra \"CD\"\n\tlanguage \"en\"\n\tplatform \"pc\"\n\tsourcefile \"sky\"\n\tengine \"sky\"\n\trom ( name \"sky.dnr\" size 80 md5-5000 {} )\n\trom ( name \"sky.dsk\" size -1 md5-5000 t:{} )\n)\n",
            md5_hex(&dnr),
            md5_hex(&dsk[dsk.len() - 5000..])
        ),
    )
    .unwrap();
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let scummvm = bin.join("scummvm");
    std::fs::write(
        &scummvm,
        format!(
            "#!/bin/sh\ncase \"$1\" in\n--version) echo 'ScummVM 2.9.0 (Dec 2024)' ;;\n--dump-all-detection-entries) cat '{}' ;;\nesac\n",
            list.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&scummvm, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:/usr/bin:/bin", bin.display());
    let db = tmp.path().join("romforge.db");
    let roms = tmp.path().join("roms");

    let (_, said) = cli(&db, &path, &["scan", "-q", roms.to_str().unwrap()]);
    assert!(said.contains("updated ScummVM's list"), "{said}");
    let (games, _) = cli(&db, &path, &["games"]);
    assert!(games.contains("Beneath a Steel Sky"), "{games}");
    assert!(games.contains("1 rows;"), "{games}");
}
