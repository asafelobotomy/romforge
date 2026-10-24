//! The 3DS and Wii U emulators: what each starts and what it refuses.

use super::*;

/// Azahar installs a CIA rather than starting it: Play says so, and starts
/// a cartridge dump.
#[test]
fn a_3ds_cia_is_refused_and_a_cartridge_starts() {
    let t = tempfile::tempdir().unwrap();
    let flatpak = t.path().join("flatpak");
    std::fs::create_dir_all(flatpak.join("org.azahar_emu.Azahar")).unwrap();
    let home = crate::home::Home::create(&t.path().join("parent")).unwrap();
    let env = Env {
        flatpak_dirs: vec![flatpak],
        home: Some(home),
        ..Default::default()
    };
    let conn = crate::db::open_in_memory().unwrap();
    let play = |f: &str| {
        start(
            &conn,
            &env,
            "Game",
            "Nintendo 3DS",
            &[(t.path().join(f), String::new())],
            None,
        )
    };
    let err = play("g/Game.cia").unwrap_err().to_string();
    assert!(err.contains("install it from the emulator's File menu"), "{err}");
    assert!(play("g/Game.zcia").is_err());
    let spec = play("g/Game.3ds").unwrap();
    assert_eq!(spec.emulator, "azahar");
    assert!(spec.args.last().unwrap().ends_with("Game.3ds"), "{:?}", spec.args);
    assert!(
        spec.args.iter().any(|a| a.starts_with("--env=XDG_DATA_HOME=")),
        "{:?}",
        spec.args
    );
}

/// Lime3DS (discontinued) starts what it opens and passes over a Z3DS
/// file, which only Azahar reads.
#[test]
fn lime3ds_starts_a_cartridge_but_not_a_z3ds_file() {
    let t = tempfile::tempdir().unwrap();
    let flatpak = t.path().join("flatpak");
    std::fs::create_dir_all(flatpak.join("io.github.lime3ds.Lime3DS")).unwrap();
    let env = Env {
        flatpak_dirs: vec![flatpak],
        ..Default::default()
    };
    let conn = crate::db::open_in_memory().unwrap();
    let play = |f: &str| {
        start(
            &conn,
            &env,
            "Game",
            "Nintendo 3DS",
            &[(t.path().join(f), String::new())],
            None,
        )
    };
    let spec = play("g/Game.3ds").unwrap();
    assert_eq!(spec.emulator, "lime3ds");
    assert!(spec.args.contains(&"-f".to_owned()), "{:?}", spec.args);
    let err = play("g/Game.zcci").unwrap_err().to_string();
    assert!(err.contains("lime3ds can't open .zcci"), "{err}");
    let citra = known("citra").unwrap();
    assert!(citra.discontinued.is_some() && citra.firmware == Some("lime3ds"));
    assert!(
        crate::obtain::install_flatpak(&env, "lime3ds")
            .unwrap_err()
            .to_string()
            .contains("discontinued")
    );
}

/// The 3DS cores keep their system files in RetroArch's save folder:
/// RetroBIOS's files for them are linked in there.
#[test]
fn the_3ds_cores_get_their_files_in_the_save_folder() {
    let t = tempfile::tempdir().unwrap();
    let home = crate::home::Home::create(&t.path().join("parent")).unwrap();
    let system = systems::by_key("Nintendo 3DS").unwrap();
    let (_, prepare) = owned::owned(&RETROARCH, &home, system, &LaunchOptions::default(), None).unwrap();
    let saves = home.saves().join("Nintendo 3DS");
    for d in ["Azahar", "Citra"] {
        assert!(
            prepare
                .links
                .contains(&(home.bios().join("RetroArch").join(d), saves.join(d))),
            "{:?}",
            prepare.links
        );
    }
    let snes = systems::by_key("SNES").unwrap();
    let (_, prepare) = owned::owned(&RETROARCH, &home, snes, &LaunchOptions::default(), None).unwrap();
    assert!(prepare.links.is_empty());
}

/// Cemu gets each Wii U form as it is: a WUA, a folder's `.rpx`, a `title.tmd`.
#[test]
fn cemu_starts_every_wii_u_form() {
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(t.path().join("flatpak/info.cemu.Cemu")).unwrap();
    let env = Env {
        flatpak_dirs: vec![t.path().join("flatpak")],
        ..Default::default()
    };
    let conn = crate::db::open_in_memory().unwrap();
    for f in ["g/Game.wua", "g/Game/code/game.rpx", "g/Game/title.tmd"] {
        let spec = start(
            &conn,
            &env,
            "Game",
            "Wii U",
            &[(t.path().join(f), String::new())],
            None,
        )
        .unwrap();
        assert!(
            spec.args.windows(2).any(|w| w[0] == "-g" && w[1].ends_with(f)),
            "{:?}",
            spec.args
        );
    }
}
