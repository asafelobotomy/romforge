use super::*;

/// A Steam folder with one user and a shortcut of the user's own (with a
/// key romforge doesn't know).
fn steam() -> (tempfile::TempDir, PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Steam");
    std::fs::create_dir_all(root.join("userdata/1234/config")).unwrap();
    std::fs::create_dir_all(root.join("config")).unwrap();
    std::fs::write(
        root.join("config/loginusers.vdf"),
        "\"users\"\n{\n\t\"76561197960266962\"\n\t{\n\t\t\"AccountName\"\t\t\"someone\"\n\t\t\"PersonaName\"\t\t\"Some One\"\n\t\t\"Timestamp\"\t\t\"1700000000\"\n\t}\n}\n",
    )
    .unwrap();
    let theirs: Map = vec![
        (
            "appid".into(),
            Value::Int(app_id("\"/usr/bin/lutris\"", "Lutris")),
        ),
        ("AppName".into(), Value::Str("Lutris".into())),
        ("Exe".into(), Value::Str("\"/usr/bin/lutris\"".into())),
        ("sortas".into(), Value::Str("lutris".into())),
        (
            "tags".into(),
            Value::Map(vec![("0".into(), Value::Str("favorite".into()))]),
        ),
    ];
    let top: Map = vec![(
        "shortcuts".into(),
        Value::Map(vec![("0".into(), Value::Map(theirs))]),
    )];
    std::fs::write(root.join("userdata/1234/config/shortcuts.vdf"), vdf::write(&top)).unwrap();
    (t, root)
}

#[test]
fn binary_vdf_round_trips_and_keeps_unknown_keys() {
    let m: Map = vec![(
        "shortcuts".into(),
        Value::Map(vec![(
            "0".into(),
            Value::Map(vec![
                ("appid".into(), Value::Int(0x8123_4567)),
                ("AppName".into(), Value::Str("Ünïcode".into())),
                ("whatever".into(), Value::Int(7)),
                ("tags".into(), Value::Map(vec![])),
            ]),
        )]),
    )];
    let bytes = vdf::write(&m);
    assert_eq!(&bytes[..11], b"\x00shortcuts\x00");
    assert_eq!(
        &bytes[bytes.len() - 3..],
        [0x08, 0x08, 0x08],
        "maps end, then the file's own map"
    );
    assert_eq!(vdf::read(&bytes).unwrap(), m);
    assert!(vdf::read(b"\x00shortcuts\x00\x01name").is_err(), "cut short");
}

#[test]
fn the_app_id_is_steams() {
    // crc32("\"/usr/bin/lutris\"Lutris") | 0x80000000, as BoilR and SRM work it out.
    let mut h = crc32fast::Hasher::new();
    h.update(b"\"/usr/bin/lutris\"Lutris");
    assert_eq!(
        app_id("\"/usr/bin/lutris\"", "Lutris"),
        h.finalize() | 0x8000_0000
    );
    assert!(app_id("\"/a\"", "romforge") >= 0x8000_0000);
}

#[test]
fn users_are_found_most_recent_first() {
    let (_t, root) = steam();
    std::fs::create_dir_all(root.join("userdata/999/config")).unwrap();
    let u = users(&root);
    assert_eq!(
        u.iter().map(|u| u.account.as_str()).collect::<Vec<_>>(),
        ["1234", "999"]
    );
    assert_eq!(u[0].name, "Some One");
    assert!(u[0].recent && !u[1].recent);
}

#[test]
fn romforge_is_added_once_and_removed_leaving_the_rest() {
    let (_t, root) = steam();
    let exe = Path::new("/opt/romforge/romforge.AppImage");
    let p = plan(&root, "1234", exe).unwrap();
    assert!(!p.replaces);
    assert_eq!(p.others, 1);
    assert_eq!(p.exe, "\"/opt/romforge/romforge.AppImage\"");
    assert_eq!(p.app_id, app_id(&p.exe, APP_NAME));
    assert!(
        apply_unless(&p, true)
            .unwrap_err()
            .to_string()
            .contains("close it first"),
        "never while Steam runs"
    );
    apply_unless(&p, false).unwrap();
    apply_unless(&plan(&root, "1234", exe).unwrap(), false).unwrap();

    let file = root.join("userdata/1234/config/shortcuts.vdf");
    let all = entries(&vdf::read(&std::fs::read(&file).unwrap()).unwrap());
    assert_eq!(all.len(), 2, "added once, not twice");
    assert_eq!(
        vdf::get_str(&all[0], "sortas"),
        Some("lutris"),
        "theirs kept as it was"
    );
    let ours = &all[1];
    assert_eq!(vdf::get_str(ours, "LaunchOptions"), Some("--fullscreen"));
    assert_eq!(vdf::get_str(ours, "StartDir"), Some("\"/opt/romforge\""));
    assert_eq!(vdf::get(ours, "appid"), Some(&Value::Int(p.app_id)));
    for f in &p.files {
        assert!(f.is_file(), "{}", f.display());
    }
    assert!(root.join("userdata/1234/config").join(BACKUP).is_file());
    let t = std::fs::read_to_string(root.join("controller_base/templates").join(template::FILE)).unwrap();
    let tpl = vdf::read_text(&t).unwrap();
    let Some(Value::Map(m)) = vdf::get(&tpl, "controller_mappings") else {
        panic!("{t}")
    };
    assert_eq!(vdf::get_str(m, "controller_type"), Some("controller_neptune"));

    // A new place for romforge (another AppImage): the same entry, moved.
    let moved = plan(&root, "1234", Path::new("/home/me/Apps/romforge.AppImage")).unwrap();
    assert!(moved.replaces);
    apply_unless(&moved, false).unwrap();
    let all = entries(&vdf::read(&std::fs::read(&file).unwrap()).unwrap());
    assert_eq!(all.len(), 2);

    assert!(remove_unless(&root, "1234", false).unwrap());
    let all = entries(&vdf::read(&std::fs::read(&file).unwrap()).unwrap());
    assert_eq!(all.len(), 1);
    assert_eq!(vdf::get_str(&all[0], "AppName"), Some("Lutris"));
    assert!(
        !root
            .join("controller_base/templates")
            .join(template::FILE)
            .exists()
    );
    assert!(
        !grid(&root, "1234")
            .join(format!("{}p.png", moved.app_id))
            .exists()
    );
    assert!(
        !remove_unless(&root, "1234", false).unwrap(),
        "nothing left to take out"
    );
}

#[test]
fn steam_running_is_seen_in_proc() {
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(t.path().join("123")).unwrap();
    std::fs::write(t.path().join("123/comm"), "bash\n").unwrap();
    assert!(!find::running_in(t.path()));
    std::fs::create_dir_all(t.path().join("456")).unwrap();
    std::fs::write(t.path().join("456/comm"), "steam\n").unwrap();
    assert!(find::running_in(t.path()));
}
