use super::*;

fn put(p: &Path) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, b"12345").unwrap();
}

#[test]
fn finds_serials_as_games_store_them() {
    assert_eq!(serial_in("BASLUS-00892BONUS/icon").as_deref(), Some("SLUS-00892"));
    assert_eq!(serial_in("BASLUS-20062").as_deref(), Some("SLUS-20062"));
    assert_eq!(
        serial_in("ULUS10041_DATA00/PARAM.SFO").as_deref(),
        Some("ULUS-10041")
    );
    assert_eq!(serial_in("BLUS30443-SAVE00").as_deref(), Some("BLUS-30443"));
    assert_eq!(serial_in("Silent Hill (USA).srm"), None);
    assert_eq!(serial_in("SLUS-008921"), None, "six digits are not five");
}

#[test]
fn walks_saves_but_not_backups_or_unfinished_copies() {
    let dir = tempfile::tempdir().unwrap();
    put(&dir.path().join("PlayStation/Game.srm"));
    put(&dir.path().join("PlayStation/backup/rpcs3/x/save"));
    put(&dir.path().join("PlayStation/.Game.part"));
    put(&dir.path().join("PlayStation/Game.part"));
    put(&dir.path().join("PSP/ULUS10041/PARAM.SFO"));
    let found = walk(dir.path());
    let names: Vec<_> = found
        .iter()
        .map(|s| format!("{}/{}", s.system, s.file.display()))
        .collect();
    assert_eq!(
        names,
        ["PSP/ULUS10041/PARAM.SFO", "PlayStation/Game.srm"].map(|s| s.to_owned()),
        "{names:?}"
    );
}

#[test]
fn matches_only_when_exactly_one_game_has_the_evidence() {
    let mut saves = vec![
        save("PSP", "ULUS10041/PARAM.SFO"),
        save("PSP", "ULUS99999/PARAM.SFO"),
        save("SNES", "Wild Guns (USA).srm"),
        save("SNES", "Twin (USA).srm"),
        save("SNES", "Nothing (USA).srm"),
    ];
    let mut known = Known::default();
    let add = |m: &mut BTreeMap<(String, String), BTreeSet<String>>, sys: &str, key: &str, t: &str| {
        m.entry((sys.into(), key.into())).or_default().insert(t.into());
    };
    add(&mut known.serials, "PSP", "ULUS10041", "Game A (USA)");
    add(&mut known.serials, "PSP", "ULUS99999", "Game B (USA)");
    add(&mut known.serials, "PSP", "ULUS99999", "Game B (Rev 1)");
    add(&mut known.stems, "SNES", "Wild Guns (USA)", "Wild Guns (USA)");
    add(&mut known.stems, "SNES", "Twin (USA)", "One");
    add(&mut known.stems, "SNES", "Twin (USA)", "Other");
    match_all(&mut saves, &known);
    let got: Vec<_> = saves.iter().map(|s| (s.game.as_deref(), s.matched_by)).collect();
    assert_eq!(
        got,
        [
            (Some("Game A (USA)"), Some(MatchedBy::Serial)),
            (None, None),
            (Some("Wild Guns (USA)"), Some(MatchedBy::FileName)),
            (None, None),
            (None, None),
        ]
    );
}

fn save(system: &str, file: &str) -> Save {
    Save {
        system: system.into(),
        file: file.into(),
        path: file.into(),
        size: 1,
        modified: 0,
        serial: serial_in(file),
        game: None,
        matched_by: None,
    }
}
