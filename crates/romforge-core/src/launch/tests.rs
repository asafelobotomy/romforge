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
    let (p, a, _) = command(&Runner::Flatpak("org.x.Y".into()), vec!["{rom}".into()], &[], &[]);
    assert_eq!(p, PathBuf::from("flatpak"));
    assert_eq!(a, ["run", "org.x.Y", "{rom}"]);
    let (_, a, _) = command(
        &Runner::Flatpak("org.x.Y".into()),
        vec!["/m/g/a.zip".into()],
        &["/m/g".into(), "/h/RomForge".into()],
        &[("XDG_CONFIG_HOME".into(), "/h/c".into())],
    );
    assert_eq!(
        a,
        [
            "run",
            "--filesystem=/m/g",
            "--filesystem=/h/RomForge",
            "--env=XDG_CONFIG_HOME=/h/c",
            "org.x.Y",
            "/m/g/a.zip"
        ]
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
    assert_eq!(env.known(duck), Some(apps));
    // A Flatpak before either.
    env.flatpak_dirs = vec![t.join("flatpak")];
    std::fs::create_dir_all(t.join("flatpak/org.duckstation.DuckStation")).unwrap();
    assert_eq!(
        env.known(duck),
        Some(Runner::Flatpak("org.duckstation.DuckStation".into()))
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
