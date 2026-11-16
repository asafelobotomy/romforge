//! Milestone 27, stage 2 in the app: the Saves page and a game's saves.
//! Always on copies in temp folders.

use romforge_core::home::Home;
use romforge_core::saves::index::MatchedBy;
use romforge_core::{Library, Scope};

mod common;
use common::fixtures;

#[test]
fn saves_page_groups_by_system_and_links_matched_games() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(&tmp.path().join("parent")).unwrap();
    let roms = tmp.path().join("roms");
    std::fs::create_dir_all(&roms).unwrap();
    std::fs::copy(fixtures().join("roms/gb/Test GB.gb"), roms.join("Test GB.gb")).unwrap();

    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    let gb = home.saves().join("Game Boy");
    std::fs::create_dir_all(&gb).unwrap();
    std::fs::write(gb.join("Test GB.srm"), [0u8; 32]).unwrap();
    std::fs::write(gb.join("Nobody.srm"), [0u8; 8]).unwrap();
    // A copy of a save that lives elsewhere: not a save of its own.
    std::fs::create_dir_all(gb.join("backup/x")).unwrap();
    std::fs::write(gb.join("backup/x/Test GB.srm"), [0u8; 32]).unwrap();
    let odd = home.saves().join("Not A System");
    std::fs::create_dir_all(&odd).unwrap();
    std::fs::write(odd.join("thing.sav"), [1u8; 4]).unwrap();

    let page = lib.saves_page().unwrap();
    let tabs: Vec<(&str, &str, usize, usize)> = page
        .iter()
        .map(|t| (t.system.as_str(), t.name.as_str(), t.saves.len(), t.matched))
        .collect();
    assert_eq!(
        tabs,
        [
            ("Game Boy", "Nintendo Game Boy", 2, 1),
            ("Not A System", "Not A System", 1, 0)
        ]
    );
    let test_gb = page[0]
        .saves
        .iter()
        .find(|s| s.save.file.to_str() == Some("Test GB.srm"))
        .unwrap();
    assert_eq!(test_gb.save.game.as_deref(), Some("Test GB"));
    assert_eq!(test_gb.save.matched_by, Some(MatchedBy::FileName));
    let row = test_gb.row.clone().expect("linked to its row");
    let nobody = page[0].saves.iter().find(|s| s.save.game.is_none()).unwrap();
    assert_eq!(nobody.row, None, "not matched, never guessed");
    assert_eq!(nobody.box_aspect, 0);
    assert!(test_gb.box_aspect > 0, "its row's box shape");
    assert_eq!(test_gb.cover, None, "no box art fetched in this library");

    let mine = lib.game_saves(&row).unwrap();
    assert_eq!(mine.count, 1);
    assert_eq!(mine.latest, test_gb.save.modified);
    assert!(mine.latest > 0);
}

#[test]
fn no_saves_folder_means_an_empty_page() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(&tmp.path().join("parent")).unwrap();
    let lib = Library::open(home.db()).unwrap();
    let _ = std::fs::remove_dir_all(home.saves());
    assert!(lib.saves_page().unwrap().is_empty());
}
