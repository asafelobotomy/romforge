use super::*;

#[test]
fn known_emulators_cover_the_systems_table() {
    for s in systems::SYSTEMS {
        for e in s.emulators {
            assert!(
                KNOWN_EMULATORS.iter().any(|k| k.name == *e),
                "{} lists unknown {e}",
                s.key
            );
        }
    }
    for e in std::iter::once(&RETROARCH).chain(KNOWN_EMULATORS) {
        assert!(
            e.args.iter().any(|a| a.contains("{rom}") || a.contains("{set}")),
            "{}",
            e.name
        );
        assert!(
            e.config.is_empty() || e.config.iter().any(|a| a.contains("{config}")),
            "{}: config flags without {{config}}",
            e.name
        );
        assert!(
            e.fullscreen.iter().all(|a| !a.contains('{')),
            "{}: fullscreen flags take no file",
            e.name
        );
        assert_eq!(known(&e.name.to_uppercase()), Some(e), "found by name");
    }
}

#[test]
fn flatpak_commands() {
    let flatpak = |command: Option<&str>| Runner::Flatpak {
        id: "org.x.Y".into(),
        command: command.map(str::to_owned),
    };
    let (p, a, _) = command(&flatpak(Some("y-qt")), vec!["{rom}".into()], &[], &[]);
    assert_eq!(p, PathBuf::from("flatpak"));
    assert_eq!(a, ["run", "org.x.Y", "{rom}"], "nothing to set: as it is");
    let grants = ["/m/g".into(), "/h/RomForge".into()];
    let env = [("XDG_CONFIG_HOME".to_owned(), "/h/c".to_owned())];
    let (_, a, _) = command(&flatpak(Some("y-qt")), vec!["/m/g/a.zip".into()], &grants, &env);
    assert_eq!(
        a,
        [
            "run",
            "--filesystem=/m/g",
            "--filesystem=/h/RomForge",
            "--command=env",
            "org.x.Y",
            "XDG_CONFIG_HOME=/h/c",
            "y-qt",
            "/m/g/a.zip"
        ],
        "Flatpak sets its XDG folders over --env="
    );
    let (_, a, _) = command(&flatpak(None), vec!["/m/g/a.zip".into()], &grants, &env);
    assert_eq!(
        a,
        [
            "run",
            "--filesystem=/m/g",
            "--filesystem=/h/RomForge",
            "--env=XDG_CONFIG_HOME=/h/c",
            "org.x.Y",
            "/m/g/a.zip"
        ],
        "its command unknown"
    );
    let env = [("MEDNAFEN_HOME".to_owned(), "/h/m".to_owned())];
    let (p, a, e) = command(
        &Runner::Program("/bin/x".into()),
        vec!["r".into()],
        &["/m".into()],
        &env,
    );
    assert_eq!(
        (p, a),
        (PathBuf::from("/bin/x"), vec!["r".to_owned()]),
        "a plain program needs no grants"
    );
    assert_eq!(e, env, "its environment is set by spawn");
    let vars = Vars {
        rom: Path::new("/r/a b.iso"),
        set: "a b",
        core: Some(Path::new("/c/x_libretro.so")),
        config: None,
        bios: None,
        system: "GBA",
    };
    assert_eq!(fill(&["-e", "{rom}"], &vars), ["-e", "/r/a b.iso"]);
    assert_eq!(
        known_args(&RETROARCH, &vars, &LaunchOptions::default()),
        ["-f", "-L", "/c/x_libretro.so", "/r/a b.iso"],
        "fullscreen first"
    );
    let set = LaunchOptions {
        windowed: true,
        args: vec!["--verbose".into()],
        ..LaunchOptions::default()
    };
    assert_eq!(
        known_args(&RETROARCH, &vars, &set),
        ["--verbose", "-L", "/c/x_libretro.so", "/r/a b.iso"],
        "windowed, extra arguments before the game's"
    );
    let owned = Vars {
        config: Some(Path::new("/h/RomForge/Emulators/config/retroarch")),
        ..vars
    };
    assert_eq!(
        known_args(&RETROARCH, &owned, &LaunchOptions::default())[1..5],
        [
            "--config",
            "/h/RomForge/Emulators/config/retroarch/retroarch.cfg",
            "--appendconfig",
            "/h/RomForge/Emulators/config/retroarch/systems/GBA.cfg"
        ],
        "romforge's config after the fullscreen flag"
    );
}

fn program(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn find_rules_in_order() {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path();
    let duck = known("duckstation").unwrap();
    let mut env = Env::default().with_emulators_dir(&t.join("RomForge/Emulators"));
    env.emulator_dirs.insert(0, t.join("Applications"));
    assert_eq!(env.known(duck), None);

    // An AppImage in RomForge/Emulators, any case; the newest name wins.
    program(&t.join("RomForge/Emulators/DuckStation-1.AppImage"));
    program(&t.join("RomForge/Emulators/duckstation-2.appimage"));
    std::fs::write(t.join("RomForge/Emulators/duckstation-3.AppImage"), "").unwrap();
    let image2 = Runner::Program(t.join("RomForge/Emulators/duckstation-2.appimage"));
    assert_eq!(env.known(duck), Some(image2), "not executable: skipped");
    // ~/Applications comes first.
    program(&t.join("Applications/DuckStation-x64.AppImage"));
    let apps = Runner::Program(t.join("Applications/DuckStation-x64.AppImage"));
    assert_eq!(env.known(duck), Some(apps.clone()));
    // A Flatpak before either.
    env.flatpak_dirs = vec![t.join("flatpak")];
    std::fs::create_dir_all(t.join("flatpak/org.duckstation.DuckStation")).unwrap();
    assert_eq!(
        env.known(duck),
        Some(apps.clone()),
        "where it came from unknown: not used"
    );
    flatpak::fake(
        &t.join("flatpak"),
        "org.duckstation.DuckStation",
        flatpak::FLATHUB_URL,
    );
    assert_eq!(
        env.known(duck),
        Some(Runner::Flatpak {
            id: "org.duckstation.DuckStation".into(),
            command: None
        })
    );
    let active = t.join("flatpak/org.duckstation.DuckStation/current/active");
    std::fs::create_dir_all(&active).unwrap();
    std::fs::write(
        active.join("metadata"),
        "[Application]\nname=org.duckstation.DuckStation\ncommand=duckstation-qt\n",
    )
    .unwrap();
    assert!(
        matches!(env.known(duck), Some(Runner::Flatpak { command: Some(c), .. }) if c == "duckstation-qt"),
        "its command from its metadata"
    );
    // PATH before that.
    program(&t.join("bin/duckstation-qt"));
    env.path = vec![t.join("bin")];
    assert_eq!(
        env.known(duck),
        Some(Runner::Program(t.join("bin/duckstation-qt")))
    );
    // And the path set in Settings first of all.
    program(&t.join("mine/ds"));
    env.user_paths.insert("duckstation".into(), t.join("mine/ds"));
    assert_eq!(env.known(duck), Some(Runner::Program(t.join("mine/ds"))));

    // MAME's machine entries look for MAME's AppImage.
    program(&t.join("Applications/MAME-0.289.AppImage"));
    let fm7 = known("mame-fm7").unwrap();
    assert_eq!(
        env.known(fm7),
        Some(Runner::Program(t.join("Applications/MAME-0.289.AppImage")))
    );
    // RomForge's cores come before the system's.
    assert_eq!(env.core_dirs[0], t.join("RomForge/Emulators/cores"));
}

#[test]
fn placeholders_are_filled_once() {
    let vars = Vars {
        rom: Path::new("/r/{set} {dir}.zip"),
        set: "{system}",
        core: None,
        config: None,
        bios: None,
        system: "GBA",
    };
    assert_eq!(
        fill(&["{rom}", "{set}-{system}", "{nope} {"], &vars),
        ["/r/{set} {dir}.zip", "{system}-GBA", "{nope} {"],
        "text from a value isn't filled again"
    );
}

#[cfg(unix)]
#[test]
fn a_helper_run_that_hangs_is_stopped() {
    let t = std::time::Instant::now();
    let mut slow = std::process::Command::new("sh");
    slow.args(["-c", "sleep 30"]);
    let err = tools::output_within(slow, std::time::Duration::from_millis(200)).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    assert!(t.elapsed() < std::time::Duration::from_secs(10));
    let mut quick = std::process::Command::new("sh");
    quick.args(["-c", "echo hi"]);
    let out = tools::output_within(quick, std::time::Duration::from_secs(10)).unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout, b"hi\n");
}

/// An emulator that takes its BIOS folder on the command line gets it only
/// with a RomForge folder.
#[test]
fn bios_flags_come_with_the_romforge_folder() {
    let towns = known("mame-fmtowns").unwrap();
    let mut vars = Vars {
        rom: Path::new("/g/t.chd"),
        set: "t",
        core: None,
        config: None,
        bios: None,
        system: "FM Towns",
    };
    let o = LaunchOptions::default();
    assert!(!known_args(towns, &vars, &o).contains(&"-rompath".to_owned()));
    vars.bios = Some(Path::new("/rf/Bios/mame"));
    let args = known_args(towns, &vars, &o);
    assert!(
        args.windows(2).any(|w| w == ["-rompath", "/rf/Bios/mame"]),
        "{args:?}"
    );
    let staging = known("dosbox-staging").unwrap();
    vars.bios = Some(Path::new("/rf/Bios/dosbox-staging"));
    assert!(known_args(staging, &vars, &o).contains(&"romdir=/rf/Bios/dosbox-staging/mt32-roms".to_owned()));
}

/// MAME's `-rompath` only when `Bios/mame/` holds a file: an empty folder
/// would hide the rompath of the user's own `mame.ini` (romforge's copy
/// starts from it).
#[test]
fn bios_flags_only_for_a_folder_with_files() {
    let t = tempfile::tempdir().unwrap();
    let bin = t.path().join("bin");
    program(&bin.join("mame"));
    let home = crate::home::Home::create(&t.path().join("parent")).unwrap();
    let env = Env {
        path: vec![bin],
        home: Some(home.clone()),
        ..Default::default()
    };
    let conn = crate::db::open_in_memory().unwrap();
    let files = [(t.path().join("g/Game.chd"), String::new())];
    let plan = || start(&conn, &env, "Game", "FM Towns", &files, None).unwrap();
    std::fs::create_dir_all(home.bios().join("mame/sub")).unwrap();
    let spec = plan();
    assert!(!spec.args.contains(&"-rompath".to_owned()), "{:?}", spec.args);
    std::fs::write(home.bios().join("mame/sub/fmtowns.zip"), b"set").unwrap();
    let spec = plan();
    let bios = home.bios().join("mame").display().to_string();
    assert!(
        spec.args.windows(2).any(|w| w == ["-rompath", bios.as_str()]),
        "{:?}",
        spec.args
    );
}

/// A user-added emulator's `{bios}` is `Bios/<its name>/` with a RomForge
/// folder, not "".
#[test]
fn user_emulators_get_their_bios_folder() {
    let t = tempfile::tempdir().unwrap();
    let bin = t.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("towns"), "").unwrap();
    let home = crate::home::Home::create(&t.path().join("parent")).unwrap();
    let env = Env {
        home: Some(home.clone()),
        ..Default::default()
    };
    let conn = crate::db::open_in_memory().unwrap();
    let u = UserEmulator {
        name: "towns".into(),
        system: "FM Towns".into(),
        program: bin.join("towns").display().to_string(),
        args: vec!["--bios={bios}".into(), "{rom}".into()],
        archives: false,
    };
    add_emulator(&conn, &u).unwrap();
    let files = [(t.path().join("g/Game.chd"), String::new())];
    let spec = start(&conn, &env, "Game", "FM Towns", &files, None).unwrap();
    assert_eq!(
        spec.args[0],
        format!("--bios={}", home.bios().join("towns").display())
    );
}

mod bios;
mod consoles;
mod sources;

/// MAME's copy of `mame.ini` starts from the user's own (their rompath
/// kept) and gets Emulator Settings' keys; theirs is never written.
#[test]
fn a_file_only_emulator_starts_from_the_users_settings() {
    let t = tempfile::tempdir().unwrap();
    let user = existing::UserDirs {
        home: t.path().join("u"),
        xdg: existing::Xdg {
            config: t.path().join("u/.config"),
            data: t.path().join("u/.local/share"),
        },
    };
    let theirs = user.home.join(".mame/mame.ini");
    std::fs::create_dir_all(theirs.parent().unwrap()).unwrap();
    std::fs::write(&theirs, "rompath /my/roms\nwaitvsync 0\n").unwrap();
    let home = crate::home::Home::create(&t.path().join("parent")).unwrap();
    let mut s = crate::emusettings::EmulatorSettings::default();
    s.global.insert("vsync".into(), "true".into());
    let mut env = Env::default();
    crate::emusettings::apply(&s, &mut env);
    let mame = known("mame").unwrap();
    let system = crate::systems::SYSTEMS
        .iter()
        .find(|s| s.emulators.contains(&"mame"))
        .unwrap();
    let (dir, prep) = owned::owned(mame, &home, system, &env.options(mame), Some(&user)).unwrap();
    prepare(&prep).unwrap();
    let ours = std::fs::read_to_string(dir.join("mame.ini")).unwrap();
    assert!(ours.contains("rompath /my/roms"), "{ours}");
    assert!(ours.contains("waitvsync 1"), "{ours}");
    assert_eq!(
        std::fs::read_to_string(&theirs).unwrap(),
        "rompath /my/roms\nwaitvsync 0\n"
    );
    // Once there, romforge's copy is the one kept.
    std::fs::write(&theirs, "rompath /elsewhere\n").unwrap();
    prepare(&prep).unwrap();
    assert!(
        std::fs::read_to_string(dir.join("mame.ini"))
            .unwrap()
            .contains("/my/roms")
    );
}
