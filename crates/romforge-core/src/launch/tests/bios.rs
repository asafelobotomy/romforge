//! Milestone 20, step 5: a core without a BIOS file it can't run without
//! is passed over for the next, and Play says which file is missing.

use super::*;

fn list() -> String {
    let needs = |core: &str| {
        serde_json::json!({"id": core, "profile": {
        "display_name": core, "cores": [core], "systems": ["sony-playstation"],
        "files": [
            {"name": "scph5501.bin", "required": true, "size": 4},
            {"name": "big-optional.bin", "required": false}
        ]}})
    };
    serde_json::json!({"generated_at": "2026-10-25T00:00:00Z", "items": [
        needs("mednafen_psx_hw"),
        needs("swanstation"),
        {"id": "pcsx_rearmed", "profile": {"cores": ["pcsx_rearmed"], "systems": ["sony-playstation"],
            "files": [{"name": "scph1001.bin", "required": true, "hle_fallback": true}]}}
    ]})
    .to_string()
}

#[test]
fn a_core_without_its_bios_is_passed_over() {
    let t = tempfile::tempdir().unwrap();
    let (bin, cores, db) = (t.path().join("bin"), t.path().join("cores"), t.path().join("db"));
    program(&bin.join("retroarch"));
    std::fs::create_dir_all(&cores).unwrap();
    std::fs::create_dir_all(&db).unwrap();
    std::fs::write(db.join(crate::firmware::FILE), list()).unwrap();
    let core = |c: &str| std::fs::write(cores.join(format!("{c}_libretro.so")), b"").unwrap();
    core("mednafen_psx_hw");
    core("swanstation");
    let home = crate::home::Home::create(&t.path().join("parent")).unwrap();
    let env = Env {
        path: vec![bin],
        core_dirs: vec![cores.clone()],
        home: Some(home.clone()),
        firmware_list: Some(db),
        ..Env::default()
    };
    let conn = crate::db::open_in_memory().unwrap();
    let files = [(t.path().join("g/Game.cue"), String::new())];
    let play = || start(&conn, &env, "Game", "PlayStation", &files, None);

    let err = play().unwrap_err().to_string();
    assert!(
        err.contains("retroarch:mednafen_psx_hw: no scph5501.bin (drop it on the BIOS page)"),
        "{err}"
    );
    assert!(err.contains("retroarch:swanstation: no scph5501.bin"), "{err}");

    // A core that runs without it (an HLE BIOS) starts, saying what it passed over.
    core("pcsx_rearmed");
    let spec = play().unwrap();
    assert_eq!(spec.emulator, "retroarch:pcsx_rearmed");
    assert_eq!(spec.skipped.len(), 2, "{:?}", spec.skipped);

    // With the file there, the first core again; an optional one doesn't count.
    std::fs::create_dir_all(home.bios().join("RetroArch")).unwrap();
    std::fs::write(home.bios().join("RetroArch/scph5501.bin"), b"bios").unwrap();
    let spec = play().unwrap();
    assert_eq!(spec.emulator, "retroarch:mednafen_psx_hw");
    assert!(spec.skipped.is_empty());

    // No list (never downloaded): nothing is held back.
    std::fs::remove_file(home.bios().join("RetroArch/scph5501.bin")).unwrap();
    let env = Env {
        firmware_list: None,
        ..env.clone()
    };
    let spec = start(&conn, &env, "Game", "PlayStation", &files, None).unwrap();
    assert_eq!(spec.emulator, "retroarch:mednafen_psx_hw");
}

/// Without RetroBIOS's list, an emulator that can't run without a BIOS
/// (DuckStation) and has none in its `Bios/` folder isn't started into its
/// own "no BIOS" message: Play says so.
#[test]
fn no_list_and_no_bios_holds_back_an_emulator_that_needs_one() {
    let t = tempfile::tempdir().unwrap();
    let bin = t.path().join("bin");
    program(&bin.join("duckstation-qt"));
    let home = crate::home::Home::create(&t.path().join("parent")).unwrap();
    let env = Env {
        path: vec![bin],
        home: Some(home.clone()),
        ..Env::default()
    };
    let conn = crate::db::open_in_memory().unwrap();
    let files = [(t.path().join("g/Game.cue"), String::new())];
    let play = || start(&conn, &env, "Game", "PlayStation", &files, Some("duckstation"));

    let err = play().unwrap_err().to_string();
    assert!(err.contains("duckstation: no BIOS file in"), "{err}");

    let dir = home.bios().join("duckstation");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("scph5501.bin"), b"bios").unwrap();
    assert_eq!(play().unwrap().emulator, "duckstation");
}
