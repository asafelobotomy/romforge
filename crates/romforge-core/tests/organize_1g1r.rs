//! Milestone 2 end to end, always on copies in a temp folder.
//!
//! One game, one ROM, and clone lists.

use romforge_core::organize::OrganizeOptions;
use romforge_core::{Library, Scope};

mod common;
use common::organize::*;

#[test]
fn one_game_one_rom_keeps_the_best_version() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("roms");
    std::fs::create_dir(&src).unwrap();
    let versions = [
        "Puzzle (Japan)",
        "Puzzle (USA)",
        "Puzzle (USA) (Rev 1)",
        "Puzzle (Europe)",
        "Puzzle (USA) (Beta)",
        "Other (Japan)",
    ];
    let mut games = String::new();
    for (i, name) in versions.iter().enumerate() {
        let data = vec![i as u8 + 1; 2048];
        games += &format!(
            r#"<game name="{name}">{}</game>"#,
            rom(&format!("{name}.gb"), &data)
        );
        std::fs::write(src.join(format!("{i}.gb")), &data).unwrap();
    }
    let dat = tmp.path().join("gb.dat");
    std::fs::write(
        &dat,
        format!(r#"<datafile><header><name>Nintendo - Game Boy</name></header>{games}</datafile>"#),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.scan(&src, Scope::All).unwrap();

    let mut opts = OrganizeOptions::new(&src, tmp.path().join("sorted"));
    opts.one_g1r = true;
    let names = |lib: &Library, opts: &OrganizeOptions| {
        let mut v: Vec<String> = lib
            .organize_plan(opts)
            .unwrap()
            .moves
            .into_iter()
            .map(|m| m.game)
            .collect();
        v.sort();
        v
    };
    assert_eq!(names(&lib, &opts), ["Other (Japan)", "Puzzle (USA) (Rev 1)"]);
    opts.regions = vec!["Europe".into()];
    assert_eq!(names(&lib, &opts), ["Other (Japan)", "Puzzle (Europe)"]);
    opts.one_g1r = false;
    assert_eq!(names(&lib, &opts).len(), versions.len());
}

#[test]
fn one_g1r_counts_what_is_already_organized() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    let usa = vec![1u8; 2048];
    let jpn = vec![2u8; 2048];
    std::fs::write(a.join("usa.gb"), &usa).unwrap();
    std::fs::write(b.join("jpn.gb"), &jpn).unwrap();
    let dat = tmp.path().join("gb.dat");
    std::fs::write(
        &dat,
        format!(
            r#"<datafile><header><name>Nintendo - Game Boy</name></header><game name="Puzzle (USA)">{}</game><game name="Puzzle (Japan)">{}</game></datafile>"#,
            rom("Puzzle (USA).gb", &usa),
            rom("Puzzle (Japan).gb", &jpn)
        ),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    let dest = tmp.path().join("sorted");
    let mut opts = OrganizeOptions::new(tmp.path(), &dest);
    opts.one_g1r = true;

    lib.scan(&a, Scope::All).unwrap();
    let plan = lib.organize_plan(&opts).unwrap();
    lib.organize_apply(&plan, &dest).unwrap();
    lib.scan(&dest, Scope::All).unwrap();
    // Later the Japanese version turns up: USA is already there, so it stays.
    lib.scan(&b, Scope::All).unwrap();
    let plan = lib.organize_plan(&opts).unwrap();
    assert!(plan.moves.is_empty(), "{:?}", plan.moves);
    assert!(
        plan.skipped
            .iter()
            .any(|s| s.reason == "1G1R: keeping Puzzle (USA)")
    );
}

/// A clone list makes titles that are one game under other names one title
/// for 1G1R ("Aero Fighters" in the USA is "Sonic Wings" in Japan), and its
/// priority decides between equals.
#[test]
fn clone_lists_join_retitled_games_for_1g1r() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("roms");
    std::fs::create_dir(&src).unwrap();
    let versions = [
        "Aero Fighters (USA)",
        "Sonic Wings (Japan)",
        "Angelique (Japan)",
        "Angelique - Premium Box (Japan)",
    ];
    let mut games = String::new();
    for (i, name) in versions.iter().enumerate() {
        let data = vec![i as u8 + 1; 2048];
        games += &format!(
            r#"<game name="{name}">{}</game>"#,
            rom(&format!("{name}.sfc"), &data)
        );
        std::fs::write(src.join(format!("{i}.sfc")), &data).unwrap();
    }
    let dat = tmp.path().join("snes.dat");
    std::fs::write(
        &dat,
        format!(r#"<datafile><header><name>Nintendo - Super Nintendo Entertainment System</name></header>{games}</datafile>"#),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.scan(&src, Scope::All).unwrap();
    let mut opts = OrganizeOptions::new(&src, tmp.path().join("sorted"));
    opts.one_g1r = true;
    let names = |lib: &Library| {
        let mut v: Vec<String> = lib
            .organize_plan(&opts)
            .unwrap()
            .moves
            .into_iter()
            .map(|m| m.game)
            .collect();
        v.sort();
        v
    };
    assert_eq!(names(&lib).len(), 4, "different names: four titles");
    let list = br#"{"description": {"name": "x"}, "variants": [
        {"group": "Aero Fighters", "titles": [{"searchTerm": "Aero Fighters"}, {"searchTerm": "Sonic Wings"}]},
        {"group": "Angelique - Premium Box", "titles": [{"searchTerm": "Angelique - Premium Box"}, {"searchTerm": "Angelique", "priority": 3}]}
    ]}"#;
    assert_eq!(lib.store_clone_list("SNES", "test (No-Intro)", list).unwrap(), 4);
    assert_eq!(
        names(&lib),
        ["Aero Fighters (USA)", "Angelique - Premium Box (Japan)"],
        "one per group: the best region, then the lower priority"
    );
}

/// A clone list replaces its own earlier rows (old titles go), and a title
/// another group already claimed keeps that group.
#[test]
fn clone_lists_replace_their_rows_and_keep_first_groups() {
    let mut lib = Library::open_in_memory().unwrap();
    let a = br#"{"variants": [
        {"group": "Aero Fighters", "titles": [{"searchTerm": "Aero Fighters"}, {"searchTerm": "Sonic Wings"}]},
        {"group": "Gone Game", "titles": [{"searchTerm": "Gone Game"}]}
    ]}"#;
    assert_eq!(lib.store_clone_list("SNES", "A (No-Intro)", a).unwrap(), 3);
    // Another list claiming "Sonic Wings" for another group: it stays.
    let b = br#"{"variants": [{"group": "Sonic Wings Special", "titles": [{"searchTerm": "Sonic Wings"}, {"searchTerm": "Sonic Wings Special"}]}]}"#;
    assert_eq!(
        lib.store_clone_list("SNES", "B (Redump)", b).unwrap(),
        1,
        "only the new title"
    );
    // A new version of list A without "Gone Game".
    let a2 = br#"{"variants": [{"group": "Aero Fighters", "titles": [{"searchTerm": "Aero Fighters"}, {"searchTerm": "Sonic Wings"}]}]}"#;
    assert_eq!(lib.store_clone_list("SNES", "A (No-Intro)", a2).unwrap(), 2);
    // Back to plain names for 1G1R: "Gone Game" is in no group now.
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("roms");
    std::fs::create_dir(&src).unwrap();
    let mut games = String::new();
    for (i, name) in [
        "Gone Game (USA)",
        "Gone Game (Japan)",
        "Aero Fighters (USA)",
        "Sonic Wings (Japan)",
    ]
    .iter()
    .enumerate()
    {
        let data = vec![i as u8 + 1; 2048];
        games += &format!(
            r#"<game name="{name}">{}</game>"#,
            rom(&format!("{name}.sfc"), &data)
        );
        std::fs::write(src.join(format!("{i}.sfc")), &data).unwrap();
    }
    let dat = tmp.path().join("snes.dat");
    std::fs::write(&dat, format!(r#"<datafile><header><name>Nintendo - Super Nintendo Entertainment System</name></header>{games}</datafile>"#)).unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.scan(&src, Scope::All).unwrap();
    let mut opts = OrganizeOptions::new(&src, tmp.path().join("sorted"));
    opts.one_g1r = true;
    let mut names: Vec<String> = lib
        .organize_plan(&opts)
        .unwrap()
        .moves
        .into_iter()
        .map(|m| m.game)
        .collect();
    names.sort();
    assert_eq!(names, ["Aero Fighters (USA)", "Gone Game (USA)"]);
}
