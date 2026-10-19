//! Milestone 5 with stand-in emulators: nothing real is installed or run
//! except a script that records its arguments.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use romforge_core::hash::hash_stream;
use romforge_core::launch::{Env, UserEmulator};
use romforge_core::{Library, Scope};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

struct Fake {
    _dir: tempfile::TempDir,
    root: PathBuf,
    env: Env,
}

/// An empty environment to install stand-ins into.
fn fake() -> Fake {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    for d in ["bin", "cores", "flatpak"] {
        std::fs::create_dir(root.join(d)).unwrap();
    }
    let env = Env {
        path: vec![root.join("bin")],
        core_dirs: vec![root.join("cores")],
        flatpak_dirs: vec![root.join("flatpak")],
        ..Default::default()
    };
    Fake { _dir: dir, root, env }
}

fn core(f: &Fake, name: &str) -> PathBuf {
    let p = f
        .root
        .join("cores")
        .join(format!("{name}_libretro.{}", std::env::consts::DLL_EXTENSION));
    std::fs::write(&p, b"").unwrap();
    p
}

fn library() -> Library {
    let mut lib = Library::open_in_memory().unwrap();
    for e in std::fs::read_dir(fixtures().join("dats")).unwrap() {
        lib.import_dat(e.unwrap().path(), None).unwrap();
    }
    lib.scan(fixtures().join("roms"), Scope::All).unwrap();
    lib
}

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
    // mGBA reads archives itself.
    std::fs::create_dir(f.root.join("flatpak/io.mgba.mGBA")).unwrap();
    let spec = lib.launch_plan(&f.env, "Test GBA", None).unwrap();
    assert_eq!(spec.program, PathBuf::from("flatpak"));
    // The game's folder is granted for this run (a Flatpak sees only home).
    let folder = spec.rom.parent().unwrap().display().to_string();
    assert_eq!(
        spec.args[..3],
        [
            "run".to_owned(),
            format!("--filesystem={folder}"),
            "io.mgba.mGBA".to_owned()
        ]
    );

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
    std::fs::create_dir(f.root.join("flatpak/org.duckstation.DuckStation")).unwrap();
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
    std::fs::create_dir(f.root.join("flatpak/org.duckstation.DuckStation")).unwrap();
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

/// With a RomForge folder, RetroArch runs from romforge's copy of its
/// config: written when the game starts, the user's own keys kept.
#[test]
fn retroarch_runs_from_romforge_s_own_config() {
    let lib = library();
    let mut f = fake();
    let out = f.root.join("args.txt");
    script(
        &f.root.join("bin/retroarch"),
        &format!("echo \"$@\" > '{}'", out.display()),
    );
    core(&f, "mgba");
    let home = romforge_core::home::Home::create(&f.root.join("parent")).unwrap();
    f.env.home = Some(home.clone());

    let spec = lib.launch_plan(&f.env, "Test GBA", None).unwrap();
    let config = home.emulator_config("retroarch");
    let (base, system) = (
        config.join("retroarch.cfg"),
        config.join("systems/Game Boy Advance.cfg"),
    );
    assert_eq!(
        spec.args[..5],
        [
            "-f".to_owned(),
            "--config".to_owned(),
            base.display().to_string(),
            "--appendconfig".to_owned(),
            system.display().to_string()
        ]
    );
    assert!(!base.exists(), "planning writes nothing");

    romforge_core::launch::spawn(&spec).unwrap();
    let text = std::fs::read_to_string(&base).unwrap();
    let bios = home.bios().join("RetroArch");
    assert!(
        text.contains(&format!("system_directory = \"{}\"", bios.display())),
        "{text}"
    );
    assert!(text.contains("config_save_on_exit = \"false\""));
    let saves = home.saves().join("Game Boy Advance");
    let per_system = std::fs::read_to_string(&system).unwrap();
    assert!(per_system.contains(&format!("savefile_directory = \"{}\"", saves.display())));
    assert!(saves.is_dir() && home.states().join("Game Boy Advance").is_dir() && bios.is_dir());

    // A key set by hand in romforge's copy stays; romforge's own come back.
    let edited = text.replace(
        "config_save_on_exit = \"false\"",
        "config_save_on_exit = \"true\"",
    );
    std::fs::write(&base, format!("video_driver = \"vulkan\"\n{edited}")).unwrap();
    romforge_core::launch::spawn(&spec).unwrap();
    let text = std::fs::read_to_string(&base).unwrap();
    assert!(text.starts_with("video_driver = \"vulkan\"\n"), "{text}");
    assert!(text.contains("config_save_on_exit = \"false\""));
}
