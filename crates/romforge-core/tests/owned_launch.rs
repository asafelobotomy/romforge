//! Milestone 18, 1.3 for standalone emulators: with a RomForge folder, an
//! emulator starts from romforge's own data folder (stand-in programs).
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use romforge_core::launch::{Env, spawn};
use romforge_core::{Library, Scope};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

#[test]
fn mgba_runs_from_romforge_s_folder_with_its_bios() {
    let mut lib = Library::open_in_memory().unwrap();
    for e in std::fs::read_dir(fixtures().join("dats")).unwrap() {
        lib.import_dat(e.unwrap().path(), None).unwrap();
    }
    lib.scan(fixtures().join("roms"), Scope::All).unwrap();
    let t = tempfile::tempdir().unwrap();
    let bin = t.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let out = t.path().join("env.txt");
    std::fs::write(
        bin.join("mgba-qt"),
        format!("#!/bin/sh\necho \"$XDG_CONFIG_HOME\" > '{}'\n", out.display()),
    )
    .unwrap();
    std::fs::set_permissions(bin.join("mgba-qt"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let home = romforge_core::home::Home::create(&t.path().join("parent")).unwrap();
    std::fs::create_dir_all(home.bios().join("mgba")).unwrap();
    std::fs::write(home.bios().join("mgba/gba_bios.bin"), b"bios").unwrap();
    let env = Env {
        path: vec![bin],
        home: Some(home.clone()),
        ..Default::default()
    };

    let spec = lib.launch_plan(&env, "Test GBA", Some("mgba")).unwrap();
    let dir = home.emulator_config("mgba");
    assert_eq!(
        spec.env,
        [("XDG_CONFIG_HOME".to_owned(), dir.display().to_string())]
    );
    assert_eq!(spec.args[0], "-f", "its fullscreen flag, no config flag");
    spawn(&spec).unwrap();
    let ini = std::fs::read_to_string(dir.join("mgba/config.ini")).unwrap();
    assert!(
        ini.contains(&format!(
            "gba.bios = {}",
            home.bios().join("mgba/gba_bios.bin").display()
        )),
        "{ini}"
    );
    // The stand-in saw the environment (it may still be starting).
    for _ in 0..50 {
        if std::fs::read_to_string(&out).is_ok_and(|s| !s.trim().is_empty()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(
        std::fs::read_to_string(&out).unwrap().trim(),
        dir.display().to_string()
    );
}
