//! Files only whose name fits: offered where their folders point, and never
//! where a file matched by its contents goes.

use super::*;

#[test]
fn a_name_only_file_goes_where_its_folders_point_and_never_over_an_accepted_one() {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(t.path()).unwrap();
    let dir = t.path().join("db");
    std::fs::create_dir_all(&dir).unwrap();
    let wiiu = b"wii-u keys".to_vec();
    let ds = b"3ds keys".to_vec();
    let ipl = |r: &str| serde_json::json!({"name": "IPL.bin", "path": format!("GC/{r}/IPL.bin")});
    let doc = serde_json::json!({"items": [
        {"id": "cemu", "profile": {"systems": ["nintendo-wiiu"], "files": [{"name": "keys.txt"}]}},
        {"id": "azahar", "profile": {"cores": ["azahar"], "systems": ["nintendo-3ds"],
            "files": [{"name": "keys.txt", "path": "sysdata/keys.txt", "sha1": sha1(&ds)}]}},
        {"id": "dolphin", "profile": {"cores": ["dolphin"], "systems": ["nintendo-gamecube"],
            "files": [ipl("EUR"), ipl("JAP"), ipl("USA")]}}
    ]});
    std::fs::write(dir.join(FILE), doc.to_string()).unwrap();
    let drop = t.path().join("drop");
    for d in ["Cemu", "azahar", "GC/JAP"] {
        std::fs::create_dir_all(drop.join(d)).unwrap();
    }
    std::fs::write(drop.join("Cemu/keys.txt"), &wiiu).unwrap();
    std::fs::write(drop.join("keys.txt"), &wiiu).unwrap();
    std::fs::write(drop.join("azahar/keys.txt"), &ds).unwrap();
    std::fs::write(drop.join("GC/JAP/IPL.bin"), b"jap").unwrap();
    std::fs::write(drop.join("IPL.bin"), b"any").unwrap();
    let paths = vec![drop.clone()];
    let env = Env::default();

    let p = plan(&dir, &home, &env, &paths, &mut |_, _, _| {}).unwrap();
    let by = |rel: &str| {
        let shown = drop.join(rel).display().to_string();
        p.files.iter().find(|f| f.shown == shown).unwrap()
    };
    let places = |rel: &str| {
        let mut v: Vec<_> = by(rel)
            .targets
            .iter()
            .map(|t| (t.place.strip_prefix(home.bios()).unwrap().to_owned(), t.state))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    };
    let new = TargetState::New;
    let claimed = TargetState::Claimed;
    assert_eq!(by("Cemu/keys.txt").matched, Match::NameOnly);
    assert_eq!(
        places("Cemu/keys.txt"),
        [("cemu/keys.txt".into(), new)],
        "Cemu's, not Azahar's"
    );
    assert_eq!(
        places("keys.txt"),
        [
            ("RetroArch/sysdata/keys.txt".into(), claimed),
            ("azahar/sysdata/keys.txt".into(), claimed),
            ("cemu/keys.txt".into(), new),
        ],
        "no folder: every place, but the accepted file fills Azahar's"
    );
    let jap: Vec<PathBuf> = places("GC/JAP/IPL.bin").into_iter().map(|(p, _)| p).collect();
    assert_eq!(
        jap,
        [
            PathBuf::from("RetroArch/GC/JAP/IPL.bin"),
            PathBuf::from("dolphin/GC/JAP/IPL.bin")
        ]
    );
    assert_eq!(places("IPL.bin").len(), 6);
    // Two name-only files for one place fill it once.
    let both = [by("Cemu/keys.txt").shown.clone(), by("keys.txt").shown.clone()];
    assert_eq!(p.fills(&both), p.new_places + 1);
    let open = p.name_only_open();
    assert!(open.contains(&both[0]) && open.contains(&both[1]), "{open:?}");
    assert_eq!(
        p.fills(&open),
        p.new_places + 1 + 6,
        "cemu's keys.txt, IPL.bin's six"
    );

    // Both Wii U files ticked: Azahar's place still gets the 3DS file.
    let also = [by("Cemu/keys.txt").shown.clone(), by("keys.txt").shown.clone()];
    let conn = crate::db::open_in_memory().unwrap();
    apply(&conn, &dir, &home, &env, &paths, &also, false, &mut |_, _, _| {}).unwrap();
    assert_eq!(
        std::fs::read(home.bios().join("azahar/sysdata/keys.txt")).unwrap(),
        ds
    );
    assert_eq!(
        std::fs::read(home.bios().join("RetroArch/sysdata/keys.txt")).unwrap(),
        ds
    );
    assert_eq!(std::fs::read(home.bios().join("cemu/keys.txt")).unwrap(), wiiu);
}
