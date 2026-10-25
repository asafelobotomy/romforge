use super::*;

fn env(t: &std::path::Path) -> Env {
    Env {
        home: Some(crate::home::Home::create(&t.join("parent")).unwrap()),
        ..Env::default()
    }
}

fn setting<'a>(v: &'a RawView, file: &str, key: &str) -> Option<&'a RawSetting> {
    let f = v.files.iter().find(|f| f.file == file)?;
    f.settings.iter().find(|s| s.key == key)
}

const INI: &str = "duckstation/settings.ini";

#[test]
fn every_key_is_listed_and_romforges_own_are_locked() {
    let t = tempfile::tempdir().unwrap();
    let env = env(t.path());
    let conn = crate::db::open_in_memory().unwrap();
    let v = view(&conn, &env, "duckstation").unwrap();
    let f = v.files.iter().find(|f| f.file == INI).unwrap();
    assert!(
        !f.exists && f.settings.is_empty(),
        "not started from romforge yet"
    );
    assert!(!v.undo && !v.write_defaults);

    let dir = env.home.as_ref().unwrap().emulator_config("duckstation");
    std::fs::create_dir_all(dir.join("duckstation")).unwrap();
    std::fs::write(
        dir.join(INI),
        "[Main]\nSetupWizardIncomplete = false\n\n[GPU]\nRenderer = Vulkan\n\n[Audio]\nBackend = Cubeb\nOutputVolume = 100\n\n[Display]\nVSync = true\n",
    )
    .unwrap();
    let v = view(&conn, &env, "duckstation").unwrap();
    let audio = setting(&v, INI, "[Audio]Backend").unwrap();
    assert_eq!(
        (audio.value.as_str(), &audio.kind, &audio.locked),
        ("Cubeb", &RawKind::Text, &None)
    );
    assert_eq!(
        setting(&v, INI, "[Audio]OutputVolume").unwrap().kind,
        RawKind::Number
    );
    let vsync = setting(&v, INI, "[Display]VSync").unwrap();
    assert!(matches!(&vsync.kind, RawKind::Bool { on, .. } if on == "true"));
    assert!(vsync.locked.as_deref().unwrap().contains("VSync setting"));
    let wizard = setting(&v, INI, "[Main]SetupWizardIncomplete").unwrap();
    assert_eq!(wizard.locked.as_deref(), Some("romforge sets it at every start"));
    assert!(view(&conn, &env, "mame-fm7").is_err(), "it shares MAME's");
}

#[test]
fn edits_are_journaled_and_undone() {
    let t = tempfile::tempdir().unwrap();
    let env = env(t.path());
    let mut conn = crate::db::open_in_memory().unwrap();
    let dir = env.home.as_ref().unwrap().emulator_config("duckstation");
    std::fs::create_dir_all(dir.join("duckstation")).unwrap();
    let before = "[Audio]\nBackend = Cubeb\n";
    std::fs::write(dir.join(INI), before).unwrap();
    let ed = |key: &str, value: Option<&str>| RawEdit {
        file: INI.into(),
        key: key.into(),
        value: value.map(str::to_owned),
    };
    edit(
        &mut conn,
        &env,
        "duckstation",
        &[
            ed("[Audio]Backend", Some("SDL")),
            ed("[Audio]OutputVolume", Some(" 80 ")),
        ],
    )
    .unwrap();
    let text = std::fs::read_to_string(dir.join(INI)).unwrap();
    assert!(
        text.contains("Backend = SDL") && text.contains("OutputVolume = 80"),
        "{text}"
    );

    let err = |edits: &[RawEdit]| {
        edit(
            &mut crate::db::open_in_memory().unwrap(),
            &env,
            "duckstation",
            edits,
        )
    };
    let locked = err(&[ed("[GPU]Renderer", Some("OpenGL"))])
        .unwrap_err()
        .to_string();
    assert!(locked.contains("Renderer setting"), "{locked}");
    let wrong_file = RawEdit {
        file: "../elsewhere.ini".into(),
        ..ed("[A]b", Some("c"))
    };
    assert!(err(&[wrong_file]).is_err(), "only files romforge keeps");
    assert!(err(&[ed("[A]b\nc", Some("d"))]).is_err());

    assert!(view(&conn, &env, "duckstation").unwrap().undo);
    assert_eq!(undo(&conn, &env, "duckstation").unwrap(), 2);
    let text = std::fs::read_to_string(dir.join(INI)).unwrap();
    assert!(
        text.contains("Backend = Cubeb") && !text.contains("OutputVolume"),
        "{text}"
    );
    assert!(!view(&conn, &env, "duckstation").unwrap().undo);
    assert!(undo(&conn, &env, "duckstation").is_err(), "nothing left");
}
