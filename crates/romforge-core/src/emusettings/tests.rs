use super::*;

fn program(dir: &std::path::Path, name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::canonicalize(p).unwrap()
}

fn values(pairs: &[(&str, &str)]) -> Values {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn settings_are_checked_stored_and_read_back() {
    let t = tempfile::tempdir().unwrap();
    let ds = program(t.path(), "DuckStation.AppImage");
    let mut conn = crate::db::open_in_memory().unwrap();
    let s = EmulatorSettings {
        global: values(&[("fullscreen", "false")]),
        emulators: BTreeMap::from([
            (
                "DuckStation".to_owned(),
                values(&[("program", ds.to_str().unwrap()), ("args", "  -fastboot   -x ")]),
            ),
            ("mame".to_owned(), values(&[("fullscreen", ""), ("args", " ")])),
        ]),
    };
    let saved = save(&mut conn, &s).unwrap();
    let want = EmulatorSettings {
        global: values(&[("fullscreen", "false")]),
        emulators: BTreeMap::from([(
            "duckstation".to_owned(),
            values(&[("program", ds.to_str().unwrap()), ("args", "-fastboot -x")]),
        )]),
    };
    assert_eq!(saved, want, "named as the table names it; empty values dropped");
    assert_eq!(load(&conn).unwrap(), want);
    // Saving replaces everything.
    save(&mut conn, &EmulatorSettings::default()).unwrap();
    assert_eq!(load(&conn).unwrap(), EmulatorSettings::default());

    let mut err = |global: Values, emulators: &[(&str, Values)]| {
        let s = EmulatorSettings {
            global,
            emulators: emulators
                .iter()
                .map(|(n, v)| ((*n).to_owned(), v.clone()))
                .collect(),
        };
        save(&mut conn, &s).unwrap_err().to_string()
    };
    assert!(err(values(&[("program", "/bin/sh")]), &[]).contains("every emulator"));
    assert!(err(values(&[("fullscreen", "yes")]), &[]).contains("not true or false"));
    let driver = [("retroarch", values(&[("video_driver", "d3d11")]))];
    assert!(err(Values::new(), &driver).contains("not one of its choices"));
    let not_its = [("pcsx2", values(&[("screen_layout", "side")]))];
    assert!(err(Values::new(), &not_its).contains("PCSX2 has no setting"));
    let lacks = [("dolphin", values(&[("renderer", "auto")]))];
    assert!(err(Values::new(), &lacks).contains("Dolphin has no Renderer \"auto\""));
    assert!(err(Values::new(), &[("nosuch", values(&[("args", "x")]))]).contains("no emulator"));
    let fm7 = [("mame-fm7", values(&[("args", "x")]))];
    assert!(
        err(Values::new(), &fm7).contains("no emulator"),
        "it shares MAME's"
    );
    let missing = [("pcsx2", values(&[("program", "/no/such/pcsx2")]))];
    assert!(err(Values::new(), &missing).contains("PCSX2: Program"));
}

#[test]
fn an_emulator_takes_the_global_value_unless_it_sets_its_own() {
    let s = EmulatorSettings {
        global: values(&[("fullscreen", "false")]),
        emulators: BTreeMap::from([
            ("retroarch".to_owned(), values(&[("fullscreen", "true")])),
            ("mame".to_owned(), values(&[("args", "-skip_gameinfo")])),
            ("ppsspp".to_owned(), values(&[("program", "/opt/ppsspp")])),
        ]),
    };
    let mut env = Env::default();
    apply(&s, &mut env);
    assert!(!env.options["retroarch"].windowed, "its own value: fullscreen");
    let mame = &env.options["mame"];
    assert!(mame.windowed, "the global value");
    assert_eq!(mame.args, ["-skip_gameinfo"]);
    assert!(env.options["dolphin"].windowed);
    assert_eq!(env.user_paths["ppsspp"], PathBuf::from("/opt/ppsspp"));
}

/// What `o` sets in `file`, and what it takes out of it.
fn in_file<'a>(o: &'a LaunchOptions, file: &str) -> (BTreeMap<&'a str, &'a str>, Vec<&'a str>) {
    let w = o.config.iter().find(|w| w.path == std::path::Path::new(file));
    let w = w.unwrap_or_else(|| panic!("nothing for {file}: {:?}", o.config));
    (
        w.keys.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect(),
        w.remove.iter().map(String::as_str).collect(),
    )
}

#[test]
fn config_settings_become_retroarch_keys() {
    let s = EmulatorSettings {
        global: values(&[("vsync", "false"), ("auto_state", "true")]),
        emulators: BTreeMap::from([("retroarch".to_owned(), values(&[("video_driver", "vulkan")]))]),
    };
    let mut env = Env::default();
    apply(&s, &mut env);
    let (set, unset) = in_file(&env.options["retroarch"], "retroarch.cfg");
    assert_eq!(
        set,
        BTreeMap::from([
            ("video_driver", "vulkan"),
            ("video_vsync", "false"),
            ("savestate_auto_save", "true"),
            ("savestate_auto_load", "true"),
        ])
    );
    assert!(unset.contains(&"fps_show"), "not set: taken out");
    assert_eq!(
        in_file(&env.options["pcsx2"], PCSX2_INI).0["[EmuCore/GS]VsyncEnable"],
        "false"
    );
}

const PCSX2_INI: &str = "PCSX2/inis/PCSX2.ini";

/// Each emulator gets a setting in its own file's words; a value it lacks
/// takes its key out.
#[test]
fn curated_settings_go_in_each_emulators_words() {
    let s = EmulatorSettings {
        global: values(&[
            ("renderer", "vulkan"),
            ("vsync", "true"),
            ("resolution", "3"),
            ("aspect", "stretch"),
        ]),
        emulators: BTreeMap::from([("dolphin".to_owned(), values(&[("show_fps", "true")]))]),
    };
    let mut env = Env::default();
    apply(&s, &mut env);
    let (ds, _) = in_file(&env.options["duckstation"], "duckstation/settings.ini");
    assert_eq!(ds["[GPU]Renderer"], "Vulkan");
    assert_eq!(ds["[Display]VSync"], "true");
    assert_eq!(ds["[Display]AspectRatio"], "Stretch To Fill");
    let dolphin = &env.options["dolphin"];
    assert_eq!(
        in_file(dolphin, "Config/Dolphin.ini").0["[Core]GFXBackend"],
        "Vulkan"
    );
    let (gfx, _) = in_file(dolphin, "Config/GFX.ini");
    assert_eq!(
        (gfx["[Hardware]VSync"], gfx["[Settings]ShowFPS"]),
        ("True", "True")
    );
    let (_, out) = in_file(&env.options["pcsx2"], PCSX2_INI);
    assert!(out.contains(&"[EmuCore/GS]OsdShowFPS"), "set for Dolphin only");
    let (fly, _) = in_file(&env.options["flycast"], "flycast/emu.cfg");
    assert_eq!(
        (
            fly["[config]pvr.rend"],
            fly["[config]rend.vsync"],
            fly["[config]rend.Resolution"]
        ),
        ("4", "yes", "1440")
    );
    let (rpcs3, out) = in_file(&env.options["rpcs3"], "rpcs3/config.yml");
    assert_eq!(rpcs3["Video/VSync Mode"], "Full");
    assert_eq!(rpcs3["Video/Resolution Scale"], "300");
    assert_eq!(rpcs3["Video/Stretch To Display Area"], "true");
    assert!(
        out.contains(&"Video/Aspect ratio"),
        "stretch isn't one of its ratios"
    );
    let (melon, out) = in_file(&env.options["melonds"], "melonDS/melonDS.toml");
    assert!(
        out.contains(&"[3D]Renderer"),
        "no Vulkan renderer: its own default"
    );
    assert_eq!(melon["[Screen]VSync"], "true");
    let (az, _) = in_file(&env.options["azahar"], "azahar-emu/qt-config.ini");
    assert_eq!(az["[Renderer]graphics_api"], "2");
    let (mame, _) = in_file(&env.options["mame"], "mame.ini");
    assert_eq!(
        (mame["video"], mame["bgfx_backend"], mame["keepaspect"]),
        ("bgfx", "vulkan", "0")
    );
}

#[test]
fn installed_emulators_get_tabs() {
    let t = tempfile::tempdir().unwrap();
    let bin = t.path().join("bin");
    program(&bin, "retroarch");
    program(&bin, "mame");
    let conn = crate::db::open_in_memory().unwrap();
    let env = Env {
        path: vec![bin.clone()],
        ..Env::default()
    };
    let v = view(&conn, &env).unwrap();
    let names: Vec<&str> = v.installed.iter().map(|e| e.name).collect();
    assert_eq!(names, ["retroarch", "mame"], "MAME's machines share its tab");
    let mame = &v.installed[1];
    assert_eq!(mame.found.as_deref(), bin.join("mame").to_str());
    assert_eq!(
        mame.systems,
        ["Arcade (MAME)", "Fujitsu FM Towns", "Fujitsu FM-7 / FM-77"]
    );
    assert_eq!(
        mame.settings,
        ["fullscreen", "program", "args", "renderer", "vsync", "aspect"]
    );
    assert_eq!(
        mame.choices["aspect"],
        ["auto", "4:3", "stretch"],
        "the ones it has"
    );
    assert!(!mame.choices.contains_key("vsync"));
    let missing = |l: &str| v.not_installed.iter().find(|n| n.label == l);
    assert!(missing("DuckStation").is_some_and(|n| n.flatpak == Some("Flathub")));
    assert!(
        missing("Mednafen").is_some_and(|n| n.flatpak.is_none()),
        "not on Flathub"
    );
    assert!(
        missing("RPCS3").is_some_and(|n| n.flatpak.is_none()),
        "Flathub's isn't its developers'"
    );
    assert!(missing("Dolphin").is_some_and(|n| n.flatpak == Some("its developers' own repository")));
    assert!(missing("MAME: FM-7").is_none());
}
