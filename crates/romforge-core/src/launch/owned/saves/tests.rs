use super::super::prepare;
use super::*;
use crate::systems::by_key;

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

/// `name`'s writes once pointed for `system`, applied, and `file` read.
fn run(name: &str, home: &Home, system: &str, rom: &Path, dir: &Path, file: &str) -> String {
    let mut p = Prepare::default();
    point(name, home, by_key(system).unwrap(), rom, dir, None, &mut p);
    prepare(&p).unwrap();
    std::fs::read_to_string(dir.join(file)).unwrap_or_default()
}

#[test]
fn card_emulators_move_to_saves_once_their_folder_is_empty() {
    let (_t, home, dir, rom) = setup();
    let ps1 = home.saves().join("PlayStation");
    let ini = run(
        "duckstation",
        &home,
        "PlayStation",
        &rom,
        &dir,
        "duckstation/settings.ini",
    );
    assert!(ini.contains(&format!("Directory = {}", ps1.display())), "{ini}");
    assert!(ps1.is_dir());

    // A card where it saved until now: that place stays.
    std::fs::create_dir_all(dir.join("duckstation/memcards")).unwrap();
    std::fs::write(dir.join("duckstation/memcards/Game_1.mcd"), b"card").unwrap();
    let ini = run(
        "duckstation",
        &home,
        "PlayStation",
        &rom,
        &dir,
        "duckstation/settings.ini",
    );
    assert!(!ini.contains("Directory"), "{ini}");

    let ini = run(
        "pcsx2",
        &home,
        "PlayStation 2",
        &rom,
        &dir,
        "PCSX2/inis/PCSX2.ini",
    );
    let ps2 = home.saves().join("PlayStation 2");
    assert!(
        ini.contains(&format!("[Folders]\nMemoryCards = {}", ps2.display())),
        "{ini}"
    );
}

#[test]
fn a_game_saved_beside_its_rom_keeps_saving_there() {
    let (_t, home, dir, rom) = setup();
    let gba = home.saves().join("Game Boy Advance");
    let ini = run("mgba", &home, "Game Boy Advance", &rom, &dir, "mgba/config.ini");
    assert!(
        ini.contains(&format!("savegamePath = {}", gba.display())),
        "{ini}"
    );
    let toml = run(
        "melonds",
        &home,
        "Nintendo DS",
        &rom,
        &dir,
        "melonDS/melonDS.toml",
    );
    let ds = home.saves().join("Nintendo DS");
    assert!(
        toml.contains(&format!("[Instance0]\nSaveFilePath = \"{}\"", ds.display())),
        "{toml}"
    );

    std::fs::write(rom.with_extension("sav"), b"save").unwrap();
    let ini = run("mgba", &home, "Game Boy Advance", &rom, &dir, "mgba/config.ini");
    assert!(!ini.contains("savegamePath"), "{ini}");
}

#[test]
fn mednafen_keeps_its_folder_for_a_game_saved_there() {
    let (_t, home, dir, rom) = setup();
    let cfg = run("mednafen", &home, "Game Boy Advance", &rom, &dir, "mednafen.cfg");
    let gba = home.saves().join("Game Boy Advance");
    assert_eq!(cfg, format!("filesys.path_sav {}\n", gba.display()));

    std::fs::create_dir_all(dir.join("sav")).unwrap();
    std::fs::write(dir.join("sav/Other.0123.sav"), b"save").unwrap();
    let cfg = run("mednafen", &home, "Game Boy Advance", &rom, &dir, "mednafen.cfg");
    assert!(cfg.contains("path_sav"), "another game's save: {cfg}");
    std::fs::write(dir.join("sav/Game.4567.sav"), b"save").unwrap();
    let cfg = run("mednafen", &home, "Game Boy Advance", &rom, &dir, "mednafen.cfg");
    assert!(!cfg.contains("path_sav"), "{cfg}");
}

#[test]
fn dolphin_cards_go_to_gamecube_for_wii_games_too() {
    let (_t, home, dir, rom) = setup();
    let gc = home.saves().join("GameCube");
    let ini = run("dolphin", &home, "Wii", &rom, &dir, "Config/Dolphin.ini");
    assert!(
        ini.contains(&format!("GCIFolderAPath = {}\n", gc.display())),
        "{ini}"
    );
    assert!(
        ini.contains(&format!(
            "MemcardAPath = {}",
            gc.join("MemoryCardA.raw").display()
        )),
        "{ini}"
    );

    std::fs::create_dir_all(dir.join("GC/USA/Card A")).unwrap();
    std::fs::write(dir.join("GC/USA/Card A/01-GALE-x.gci"), b"gci").unwrap();
    let ini = run("dolphin", &home, "GameCube", &rom, &dir, "Config/Dolphin.ini");
    assert!(!ini.contains("GCIFolderAPath"), "{ini}");
}

#[test]
fn ppsspp_savedata_is_linked_unless_it_holds_saves() {
    let (_t, home, dir, rom) = setup();
    let savedata = dir.join("ppsspp/PSP/SAVEDATA");
    std::fs::create_dir_all(&savedata).unwrap();
    run("ppsspp", &home, "PSP", &rom, &dir, "");
    let to = std::fs::read_link(&savedata).unwrap();
    assert_eq!(to, std::path::absolute(home.saves().join("PSP")).unwrap());

    let (_t, home, dir, rom) = setup();
    let savedata = dir.join("ppsspp/PSP/SAVEDATA");
    std::fs::create_dir_all(savedata.join("ULUS10041000")).unwrap();
    std::fs::write(savedata.join("ULUS10041000/DATA.BIN"), b"save").unwrap();
    run("ppsspp", &home, "PSP", &rom, &dir, "");
    assert!(!savedata.is_symlink());
}

#[test]
fn flycast_by_system_and_the_users_own_keys_stay() {
    let (_t, home, dir, rom) = setup();
    let cfg = run("flycast", &home, "Dreamcast", &rom, &dir, "flycast/emu.cfg");
    let dc = home.saves().join("Dreamcast");
    assert!(
        cfg.contains(&format!("Dreamcast.VMUPath = {}", dc.display())),
        "{cfg}"
    );
    let cfg = run("flycast", &home, "Naomi", &rom, &dir, "flycast/emu.cfg");
    let naomi = home.saves().join("Naomi");
    assert!(
        cfg.contains(&format!("Dreamcast.SavePath = {}", naomi.display())),
        "{cfg}"
    );

    // Set in Emulator Settings: theirs.
    let mut p = Prepare::default();
    p.writes.push(ConfigWrite {
        path: dir.join("mgba/config.ini"),
        format: Format::Ini,
        keys: vec![("[ports.qt]savegamePath".into(), "/mine".into())],
        remove: Vec::new(),
    });
    point(
        "mgba",
        &home,
        by_key("Game Boy Advance").unwrap(),
        &rom,
        &dir,
        None,
        &mut p,
    );
    assert_eq!(
        p.writes[0].keys,
        [("[ports.qt]savegamePath".to_owned(), "/mine".to_owned())]
    );
}

#[test]
fn scummvm_and_mame_keep_a_save_folder_of_the_users() {
    let (t, home, dir, rom) = setup();
    let ini = run("mame", &home, "Arcade", &rom, &dir, "mame.ini");
    let arcade = home.saves().join("Arcade");
    assert_eq!(ini, format!("nvram_directory \"{}\"\n", arcade.display()));

    // ScummVM seeded from the user's own, which names its save folder.
    let theirs = t.path().join("theirs.ini");
    std::fs::write(&theirs, "[scummvm]\nsavepath=/home/me/saves\n").unwrap();
    let mut p = Prepare::default();
    p.seeds.push((theirs, dir.join("scummvm.ini")));
    point(
        "scummvm",
        &home,
        by_key("ScummVM").unwrap(),
        &rom,
        &dir,
        None,
        &mut p,
    );
    assert_eq!(p.writes[0].remove, ["[scummvm]savepath"]);
    assert!(p.writes[0].keys.is_empty());

    let ini = run(
        "scummvm",
        &home,
        "ScummVM",
        &rom,
        &t.path().join("cfg2"),
        "scummvm.ini",
    );
    let to = home.saves().join("ScummVM");
    assert!(ini.contains(&format!("savepath = {}", to.display())), "{ini}");
}
