use super::super::super::prepare;
use super::*;
use crate::saves::states::STATES;
use crate::systems::by_key;
use std::path::PathBuf;

/// A RomForge folder, a config folder for the emulator and a ROM.
fn setup() -> (tempfile::TempDir, Home, PathBuf, PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(&t.path().join("parent")).unwrap();
    let dir = t.path().join("cfg");
    let rom = t.path().join("games/Game.gba");
    std::fs::create_dir_all(rom.parent().unwrap()).unwrap();
    std::fs::write(&rom, b"rom").unwrap();
    (t, home, dir, rom)
}

/// `name`'s writes once its states are pointed for `system`, applied,
/// and `file` read.
fn run(name: &str, home: &Home, system: &str, rom: &Path, dir: &Path, file: &str) -> String {
    let mut p = Prepare::default();
    point(name, home, by_key(system).unwrap(), rom, dir, &mut p);
    prepare(&p).unwrap();
    std::fs::read_to_string(dir.join(file)).unwrap_or_default()
}

fn put(p: &Path) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, b"state").unwrap();
}

#[test]
fn states_go_in_the_systems_saves_folder() {
    let (_t, home, dir, rom) = setup();
    let ps1 = home.saves().join("PlayStation").join(STATES);
    let ini = run(
        "duckstation",
        &home,
        "PlayStation",
        &rom,
        &dir,
        "duckstation/settings.ini",
    );
    assert!(ini.contains(&format!("SaveStates = {}", ps1.display())), "{ini}");
    assert!(ps1.is_dir());

    let ini = run(
        "pcsx2",
        &home,
        "PlayStation 2",
        &rom,
        &dir,
        "PCSX2/inis/PCSX2.ini",
    );
    let ps2 = home.saves().join("PlayStation 2").join(STATES);
    assert!(ini.contains(&format!("Savestates = {}", ps2.display())), "{ini}");

    let gba = home.saves().join("Game Boy Advance").join(STATES);
    let ini = run("mgba", &home, "Game Boy Advance", &rom, &dir, "mgba/config.ini");
    assert!(
        ini.contains(&format!("savestatePath = {}", gba.display())),
        "{ini}"
    );
}

#[test]
fn a_state_in_the_old_place_keeps_the_emulator_there() {
    let (_t, home, dir, rom) = setup();
    put(&dir.join("duckstation/savestates/SLUS-00892_1.sav"));
    let ini = run(
        "duckstation",
        &home,
        "PlayStation",
        &rom,
        &dir,
        "duckstation/settings.ini",
    );
    assert!(!ini.contains("SaveStates"), "{ini}");

    // mGBA's beside the ROM: that game's states stay there.
    put(&rom.with_extension("ss1"));
    let ini = run("mgba", &home, "Game Boy Advance", &rom, &dir, "mgba/config.ini");
    assert!(!ini.contains("savestatePath"), "{ini}");

    // RetroArch's old States/<system>/ holding a state: everyone writes there.
    put(&home.states().join("PlayStation 2/x.state"));
    let ini = run(
        "pcsx2",
        &home,
        "PlayStation 2",
        &rom,
        &dir,
        "PCSX2/inis/PCSX2.ini",
    );
    let old = home.states().join("PlayStation 2");
    assert!(ini.contains(&format!("Savestates = {}", old.display())), "{ini}");
}

#[test]
fn ppsspp_links_its_states_folder() {
    let (_t, home, dir, rom) = setup();
    let mut p = Prepare::default();
    point("ppsspp", &home, by_key("PSP").unwrap(), &rom, &dir, &mut p);
    let to = home.saves().join("PSP").join(STATES);
    assert_eq!(p.folder_links, [(to, dir.join(PPSSPP_STATE))]);

    put(&dir.join(PPSSPP_STATE).join("ULUS10041_1.00_0.ppst"));
    let mut p = Prepare::default();
    point("ppsspp", &home, by_key("PSP").unwrap(), &rom, &dir, &mut p);
    assert!(p.folder_links.is_empty(), "its states stay where they are");
}
