//! Milestone 27, stage 2 in the app: the Saves page (a card per game with
//! its saves and save states) and a game's saves. Always on copies in temp
//! folders.

use std::path::Path;

use romforge_core::home::Home;
use romforge_core::saves::index::MatchedBy;
use romforge_core::saves::states::STATES;
use romforge_core::{Library, Scope};

mod common;
use common::fixtures;

fn put(p: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// A RomForge folder with the Game Boy fixture DAT and "Test GB" scanned.
fn library(tmp: &Path) -> (Home, Library) {
    let home = Home::create(&tmp.join("parent")).unwrap();
    let roms = tmp.join("roms");
    std::fs::create_dir_all(&roms).unwrap();
    std::fs::copy(fixtures().join("roms/gb/Test GB.gb"), roms.join("Test GB.gb")).unwrap();
    let mut lib = Library::open(home.db()).unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    (home, lib)
}

#[test]
fn saves_page_has_a_card_per_game_with_its_saves_and_states() {
    let tmp = tempfile::tempdir().unwrap();
    let (home, lib) = library(tmp.path());
    let gb = home.saves().join("Game Boy");
    put(&gb.join("Test GB.srm"), &[0; 32]);
    put(&gb.join(STATES).join("Test GB.state1"), &[0; 64]);
    put(&gb.join(STATES).join("Test GB.state1.png"), &[0; 8]);
    put(&gb.join("Nobody.srm"), &[0; 8]);
    put(&gb.join(STATES).join("Nobody.state"), &[0; 8]);
    // A copy of a save that lives elsewhere: not a save of its own.
    put(&gb.join("backup/x/Test GB.srm"), &[0; 32]);
    put(&home.saves().join("Not A System/thing.sav"), &[1; 4]);

    let page = lib.saves_page().unwrap();
    let tabs: Vec<(&str, &str, usize, usize, usize)> = page
        .iter()
        .map(|t| {
            (
                t.system.as_str(),
                t.name.as_str(),
                t.cards.len(),
                t.saves,
                t.states,
            )
        })
        .collect();
    assert_eq!(
        tabs,
        [
            ("Game Boy", "Nintendo Game Boy", 2, 2, 2),
            ("Not A System", "Not A System", 1, 1, 0)
        ]
    );
    let test_gb = page[0].cards.iter().find(|c| c.game.is_some()).unwrap();
    assert_eq!(test_gb.name, "Test GB");
    assert_eq!((test_gb.saves.len(), test_gb.states.len()), (1, 1));
    assert_eq!(test_gb.saves[0].matched_by, Some(MatchedBy::FileName));
    assert_eq!(test_gb.states[0].save.game.as_deref(), Some("Test GB"));
    assert_eq!(test_gb.states[0].slot.as_deref(), Some("1"));
    assert!(test_gb.states[0].picture.is_some());
    assert!(test_gb.box_aspect > 0, "its row's box shape");
    let row = test_gb.row.clone().expect("linked to its row");
    let nobody = page[0].cards.iter().find(|c| c.game.is_none()).unwrap();
    assert_eq!(nobody.name, "Nobody", "named after the content its files share");
    assert_eq!(
        (nobody.saves.len(), nobody.states.len(), nobody.row.clone()),
        (1, 1, None)
    );

    let mine = lib.game_saves(&row).unwrap();
    assert_eq!((mine.count, mine.states), (1, 1));
    assert!(mine.latest > 0);
}

#[test]
fn old_states_folder_moves_into_saves_and_back() {
    let tmp = tempfile::tempdir().unwrap();
    let (home, mut lib) = library(tmp.path());
    let old = home.states().join("Game Boy");
    put(&old.join("Test GB.state"), b"old");
    put(&old.join("Test GB.state.png"), b"png");
    put(
        &home.saves().join("Game Boy").join(STATES).join("Taken.state"),
        b"new",
    );
    put(&old.join("Taken.state"), b"old");
    put(&home.states().join("loose.txt"), b"?");

    let page = lib.saves_page().unwrap();
    assert_eq!(page[0].old_states, 2, "listed while still in States/");
    let plan = lib.merge_states_plan().unwrap();
    assert_eq!(plan.problem, None);
    assert_eq!(plan.moves.iter().filter(|m| !m.taken).count(), 2);
    assert_eq!(plan.unplaced.len(), 1, "a file of no system stays");
    assert!(old.join("Test GB.state").is_file(), "planning moves nothing");

    let (_, moved) = lib.merge_states().unwrap();
    assert_eq!(moved, 2);
    let new = home.saves().join("Game Boy").join(STATES);
    assert_eq!(std::fs::read(new.join("Test GB.state")).unwrap(), b"old");
    assert!(new.join("Test GB.state.png").is_file());
    assert_eq!(
        std::fs::read(new.join("Taken.state")).unwrap(),
        b"new",
        "never over a file"
    );
    assert!(old.join("Taken.state").is_file(), "that one stays");
    assert_eq!(
        romforge_core::saves::states::dir(&home, "Game Boy"),
        old,
        "so RetroArch keeps it"
    );

    assert_eq!(lib.undo_merge_states().unwrap(), 2);
    assert!(old.join("Test GB.state").is_file() && !new.join("Test GB.state").exists());
}

#[test]
fn no_saves_folder_means_an_empty_page() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(&tmp.path().join("parent")).unwrap();
    let lib = Library::open(home.db()).unwrap();
    let _ = std::fs::remove_dir_all(home.saves());
    assert!(lib.saves_page().unwrap().is_empty());
}
