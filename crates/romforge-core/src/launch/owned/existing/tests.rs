use super::super::{prepare, standalone::standalone};
use super::*;

#[test]
fn an_existing_setup_comes_across_and_goes_back() {
    let t = tempfile::tempdir().unwrap();
    let user = t.path().join("user");
    let xdg = Xdg {
        config: user.join(".config"),
        data: user.join(".local/share"),
    };
    let home = Home::create(&t.path().join("parent")).unwrap();
    let pcsx2 = crate::launch::known("pcsx2").unwrap();
    assert!(existing(pcsx2, &home, &user, &xdg).sources.is_empty());

    // The user's PCSX2: settings with a controller mapping, a memory
    // card, a cache; and a Flatpak copy too.
    let theirs = xdg.config.join("PCSX2");
    std::fs::create_dir_all(theirs.join("inis")).unwrap();
    std::fs::create_dir_all(theirs.join("memcards")).unwrap();
    std::fs::create_dir_all(theirs.join("cache")).unwrap();
    std::fs::write(theirs.join("inis/PCSX2.ini"), "[Pad1]\nType = DualShock2\n").unwrap();
    std::fs::write(theirs.join("memcards/Mcd001.ps2"), b"card").unwrap();
    std::fs::write(theirs.join("cache/gamelist.cache"), b"big").unwrap();
    let flat = user.join(".var/app/net.pcsx2.PCSX2/config/PCSX2/memcards");
    std::fs::create_dir_all(&flat).unwrap();
    std::fs::write(flat.join("Mcd001.ps2"), b"flatpak card").unwrap();

    // romforge already started it once (its own settings file), and a
    // memory card of romforge's is there.
    let dir = home.emulator_config("pcsx2");
    let bios = home.bios().join("pcsx2");
    prepare(&standalone("pcsx2", &dir, &bios).unwrap()).unwrap();

    let found = existing(pcsx2, &home, &user, &xdg);
    assert_eq!(found.sources.len(), 2);
    assert_eq!(found.files, 3, "the cache is left out");

    let conn = crate::db::open_in_memory().unwrap();
    let done = import(&conn, pcsx2, &home, &user, &xdg).unwrap();
    assert_eq!(
        (done.copied, done.kept),
        (2, 1),
        "the Flatpak's card doesn't replace the native one"
    );
    assert_eq!(
        std::fs::read(dir.join("PCSX2/memcards/Mcd001.ps2")).unwrap(),
        b"card"
    );
    assert!(!dir.join("PCSX2/cache").exists());
    let ini = dir.join("PCSX2/inis/PCSX2.ini");
    assert_eq!(
        std::fs::read_to_string(&ini).unwrap(),
        "[Pad1]\nType = DualShock2\n"
    );
    // The next start puts romforge's keys back into the user's file.
    prepare(&standalone("pcsx2", &dir, &bios).unwrap()).unwrap();
    let text = std::fs::read_to_string(&ini).unwrap();
    assert!(
        text.starts_with("[Pad1]\nType = DualShock2\n") && text.contains("SettingsVersion = 1"),
        "{text}"
    );

    // Undo: the copies go, romforge's own settings file comes back.
    assert_eq!(undo(&conn, "pcsx2").unwrap(), 2);
    assert!(!dir.join("PCSX2/memcards/Mcd001.ps2").exists());
    assert!(
        std::fs::read_to_string(&ini)
            .unwrap()
            .contains("[UI]\nSettingsVersion = 1")
    );
    assert!(undo(&conn, "pcsx2").is_err());
    assert!(
        theirs.join("memcards/Mcd001.ps2").exists(),
        "the user's own files are untouched"
    );
}
