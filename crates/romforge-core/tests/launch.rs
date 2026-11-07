//! Milestone 5 with stand-in emulators: nothing real is installed or run
//! except a script that records its arguments.
#![cfg(unix)]

use romforge_core::hash::hash_stream;
use romforge_core::launch::UserEmulator;
use romforge_core::{Library, Scope};

mod common;
use common::fixtures;
use common::launch::*;

#[test]
fn retroarch_gets_the_archive_and_the_best_installed_core() {
    let lib = library();
    let f = fake();
    let err = lib.launch_plan(&f.env, "Test GBA", None).unwrap_err().to_string();
    assert!(err.contains("RetroArch not found"), "{err}");

    script(&f.root.join("bin/retroarch"), "exit 0");
    let err = lib.launch_plan(&f.env, "Test GBA", None).unwrap_err().to_string();
    assert!(err.contains("retroarch:mgba: core not installed"), "{err}");

    let vbam = core(&f, "vbam");
    let spec = lib.launch_plan(&f.env, "Test GBA", None).unwrap();
    assert_eq!(spec.emulator, "retroarch:vbam");
    assert_eq!(spec.program, f.root.join("bin/retroarch"));
    let archive = std::fs::canonicalize(fixtures().join("roms/archives/Test GBA.zip")).unwrap();
    assert_eq!(
        spec.args,
        [
            "-f".to_owned(),
            "-L".to_owned(),
            vbam.display().to_string(),
            archive.display().to_string()
        ]
    );

    let mgba = core(&f, "mgba");
    let spec = lib.launch_plan(&f.env, "Test GBA", None).unwrap();
    assert_eq!(spec.emulator, "retroarch:mgba", "the system's first core wins");
    assert_eq!(spec.args[2], mgba.display().to_string());
    let spec = lib
        .launch_plan(&f.env, "Test GBA", Some("retroarch:vbam"))
        .unwrap();
    assert_eq!(spec.emulator, "retroarch:vbam", "asked for by name");
}

#[test]
fn standalone_emulators_never_get_a_rom_inside_an_archive() {
    let lib = library();
    let f = fake();
    // Flathub's mGBA isn't its developers': not used.
    flatpak(&f, "io.mgba.mGBA");
    let err = lib.launch_plan(&f.env, "Test GBA", None).unwrap_err().to_string();
    assert!(err.contains("mgba not installed"), "{err}");
    // Its official AppImage reads archives itself.
    let apps = f.root.join("apps");
    std::fs::create_dir(&apps).unwrap();
    script(&apps.join("mGBA-0.10.5-appimage-x64.appimage"), "exit 0");
    let mut env = f.env.clone();
    env.emulator_dirs = vec![apps.clone()];
    let spec = lib.launch_plan(&env, "Test GBA", None).unwrap();
    assert_eq!(spec.program, apps.join("mGBA-0.10.5-appimage-x64.appimage"));
    assert!(spec.rom.extension().is_some_and(|x| x == "zip"), "{:?}", spec.rom);

    // A user emulator that can't is skipped for the zipped ROM.
    let mut lib = lib;
    script(&f.root.join("bin/gbemu"), "exit 0");
    lib.add_emulator(&UserEmulator {
        name: "gbemu".into(),
        system: "Game Boy Advance".into(),
        program: "gbemu".into(),
        args: vec!["--rom".into(), "{rom}".into()],
        archives: false,
    })
    .unwrap();
    let err = lib
        .launch_plan(&f.env, "Test GBA", Some("gbemu"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("gbemu can't open archives"), "{err}");
}

#[test]
fn a_disc_starts_from_its_cue_sheet() {
    let tmp = tempfile::tempdir().unwrap();
    let track = vec![3u8; 4096];
    let cue = b"FILE \"Disc (USA).bin\" BINARY\n".to_vec();
    let h = |d: &[u8]| hash_stream(d, d.len() as u64).unwrap().raw;
    let rom = |name: &str, d: &[u8]| {
        let x = h(d);
        format!(
            r#"<rom name="{name}" size="{}" crc="{}" md5="{}" sha1="{}"/>"#,
            x.size, x.crc32, x.md5, x.sha1
        )
    };
    let dat = tmp.path().join("psx.dat");
    std::fs::write(
        &dat,
        format!(
            r#"<datafile><header><name>Sony - PlayStation</name></header><game name="Disc (USA)">{}{}</game></datafile>"#,
            rom("Disc (USA).bin", &track),
            rom("Disc (USA).cue", &cue)
        ),
    )
    .unwrap();
    let roms = tmp.path().join("roms");
    std::fs::create_dir(&roms).unwrap();
    std::fs::write(roms.join("Disc (USA).bin"), &track).unwrap();
    std::fs::write(roms.join("Disc (USA).cue"), &cue).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();

    let f = fake();
    flatpak(&f, "org.duckstation.DuckStation");
    let spec = lib.launch_plan(&f.env, "Disc", None).unwrap();
    assert_eq!(spec.emulator, "duckstation");
    let cue_path = std::fs::canonicalize(roms.join("Disc (USA).cue")).unwrap();
    assert_eq!(spec.rom, cue_path);
    assert_eq!(spec.args.last().unwrap(), &cue_path.display().to_string());
}

#[test]
fn spawns_the_emulator_with_the_rom() {
    let mut lib = library();
    let f = fake();
    let out = f.root.join("args.txt");
    script(
        &f.root.join("bin/nesemu"),
        &format!("printf '%s\\n' \"$@\" > '{}'", out.display()),
    );
    lib.add_emulator(&UserEmulator {
        name: "nesemu".into(),
        system: "NES".into(),
        program: "nesemu".into(),
        args: vec!["-fullscreen".into(), "{rom}".into()],
        archives: false,
    })
    .unwrap();
    let spec = lib.launch_plan(&f.env, "Test NES (iNES)", None).unwrap();
    assert_eq!(spec.emulator, "nesemu", "your own emulators come first");
    let pid = romforge_core::launch::spawn(&spec).unwrap();
    assert!(pid > 0);
    for _ in 0..100 {
        if out.exists() && !std::fs::read_to_string(&out).unwrap().is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let rom = std::fs::canonicalize(fixtures().join("roms/nes/Test NES (iNES).nes")).unwrap();
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        format!("-fullscreen\n{}\n", rom.display())
    );

    assert!(lib.launch_plan(&f.env, "No Such Game", None).is_err());
    let err = lib.launch_plan(&f.env, "Test", None).unwrap_err().to_string();
    assert!(err.contains("several games"), "{err}");
    assert_eq!(lib.remove_emulator("nesemu").unwrap(), 1);
}

#[test]
fn a_disc_with_its_own_cue_starts_from_that_cue() {
    let tmp = tempfile::tempdir().unwrap();
    let track = vec![4u8; 4096];
    let h = hash_stream(&track[..], track.len() as u64).unwrap().raw;
    let dat = tmp.path().join("psx.dat");
    std::fs::write(
        &dat,
        format!(
            r#"<datafile><header><name>Sony - PlayStation</name></header><game name="Disc (USA)"><rom name="Disc (USA).bin" size="{}" sha1="{}"/><rom name="Disc (USA).cue" size="5" sha1="0000000000000000000000000000000000000000"/></game></datafile>"#,
            h.size, h.sha1
        ),
    )
    .unwrap();
    let roms = tmp.path().join("roms");
    std::fs::create_dir(&roms).unwrap();
    std::fs::write(roms.join("disc.bin"), &track).unwrap();
    std::fs::write(roms.join("mine.cue"), b"FILE \"disc.bin\" BINARY\n").unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    let f = fake();
    flatpak(&f, "org.duckstation.DuckStation");
    let spec = lib.launch_plan(&f.env, "Disc (USA)", None).unwrap();
    assert_eq!(spec.rom, std::fs::canonicalize(roms.join("mine.cue")).unwrap());
}

/// Play starts a library row: exactly its files. A row that isn't
/// identified isn't started.
#[test]
fn launch_by_row() {
    let lib = library();
    let f = fake();
    script(&f.root.join("bin/retroarch"), "exit 0");
    core(&f, "mgba");
    let rows = lib.library_view().unwrap().rows;
    let gba = rows.iter().find(|r| r.title.starts_with("Test GBA")).unwrap();
    assert!(gba.playable);
    let by_row = lib.launch_row(&f.env, &gba.id, None).unwrap();
    let by_name = lib.launch_plan(&f.env, "Test GBA", None).unwrap();
    assert_eq!((by_row.rom, by_row.args), (by_name.rom, by_name.args));
    let unknown = rows.iter().find(|r| r.title == "unknown.bin").unwrap();
    assert!(!unknown.playable);
    let err = lib.launch_row(&f.env, &unknown.id, None).unwrap_err().to_string();
    assert!(err.contains("isn't identified"), "{err}");
}
