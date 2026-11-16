use super::*;

fn put(p: &Path) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, b"state").unwrap();
}

#[test]
fn slots_in_each_emulators_naming() {
    let cases = [
        ("Sonic (USA).state", Some("0")),
        ("Sonic (USA).state3", Some("3")),
        ("Sonic (USA).state.auto", Some("auto")),
        ("Pokemon Red.ss1", Some("1")),
        ("Mario Kart DS.ml2", Some("2")),
        ("Game.0123abcd.mc0", Some("0")),
        ("GALE01.s01", Some("1")),
        ("SLUS-00892_3.sav", Some("3")),
        ("SLUS-00892_resume.sav", Some("auto")),
        ("ULUS10041_1.00_2.ppst", Some("2")),
        ("SLUS-20062 (ABCD1234).01.p2s", Some("1")),
        ("SLUS-20062 (ABCD1234).resume.p2s", Some("auto")),
        ("Game.0123abcd.mca", Some("auto")),
        ("Game.srm", None),
        ("notes.txt", None),
    ];
    for (name, want) in cases {
        assert_eq!(slot(name).as_deref(), want, "{name}");
    }
}

#[test]
fn walks_states_with_their_pictures_in_both_places() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    let gb = home.saves().join("Game Boy");
    put(&gb.join("Game.srm"));
    put(&gb.join(STATES).join("Game.state1"));
    put(&gb.join(STATES).join("Game.state1.png"));
    put(&gb.join(STATES).join(".Game.state2.romforge-part"));
    put(&gb.join(STATES).join("SLUS-00892_1.sav.bak"));
    put(&home.states().join("Game Boy").join("Old.state"));
    put(&home.states().join("NES").join("Zelda.state.auto"));

    let found = walk(&home);
    let seen: Vec<(&str, String, Option<&str>, bool, bool)> = found
        .iter()
        .map(|s| {
            (
                s.save.system.as_str(),
                s.save.file.to_string_lossy().into_owned(),
                s.slot.as_deref(),
                s.picture.is_some(),
                s.old_place,
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("Game Boy", "Game.state1".into(), Some("1"), true, false),
            ("Game Boy", "Old.state".into(), Some("0"), false, true),
            ("NES", "Zelda.state.auto".into(), Some("auto"), false, true),
        ]
    );
    assert_eq!(walk_system(&home, "game boy").len(), 2);
    // The saves list doesn't take the states folder for saves.
    let saves = crate::saves::index::walk(&home.saves());
    assert_eq!(saves.len(), 1, "{saves:?}");
}

#[test]
fn emulators_keep_the_old_folder_while_it_holds_states() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    assert!(!home.states().exists(), "a new RomForge folder has no States/");
    let new = home.saves().join("NES").join(STATES);
    assert_eq!(dir(&home, "NES"), new);
    std::fs::create_dir_all(home.states().join("NES")).unwrap();
    assert_eq!(dir(&home, "NES"), new, "an empty old folder doesn't count");
    put(&home.states().join("NES/Zelda.state"));
    assert_eq!(dir(&home, "NES"), home.states().join("NES"));
}
