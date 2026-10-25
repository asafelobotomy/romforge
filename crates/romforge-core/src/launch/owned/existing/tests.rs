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
    assert!(existing(pcsx2, &home, &user, &xdg, None).sources.is_empty());

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

    let found = existing(pcsx2, &home, &user, &xdg, None);
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

/// RetroArch's own saves and states come into romforge's per-platform
/// folders: by their game, else by the core folder RetroArch sorted them
/// into; one whose platform can't be told stays where it is.
#[test]
fn retroarch_saves_go_to_their_platforms() {
    let t = tempfile::tempdir().unwrap();
    let user = t.path().join("user");
    let xdg = Xdg {
        config: user.join(".config"),
        data: user.join(".local/share"),
    };
    let home = Home::create(&t.path().join("parent")).unwrap();
    let ra = xdg.config.join("retroarch");
    std::fs::create_dir_all(ra.join("saves/Genesis Plus GX")).unwrap();
    std::fs::create_dir_all(ra.join("saves/SwanStation")).unwrap();
    std::fs::create_dir_all(ra.join("states")).unwrap();
    std::fs::write(
        ra.join("retroarch.cfg"),
        "savefile_directory = \":/saves\"\nsavestate_directory = \"~/.config/retroarch/states\"\n",
    )
    .unwrap();
    // Named by a game of the library; in a core's folder; neither.
    std::fs::write(ra.join("saves/Genesis Plus GX/Sonic (USA).srm"), b"sonic").unwrap();
    std::fs::write(ra.join("saves/SwanStation/Unknown Disc.srm"), b"psx").unwrap();
    std::fs::write(ra.join("saves/Mystery.srm"), b"?").unwrap();
    std::fs::write(ra.join("states/Sonic (USA).state1"), b"state").unwrap();
    std::fs::write(ra.join("states/Sonic (USA).state1.png"), b"png").unwrap();

    let conn = crate::db::open_in_memory().unwrap();
    let md = crate::systems::by_key("Genesis").unwrap();
    conn.execute(
        "INSERT INTO scan_roots (root_id, path, scanned_at) VALUES (1, '/g', 'now')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO files (file_id, root_id, path, size, crc32, md5, sha1, status, scanned_at)
         VALUES (1, 1, '/g/Sonic (USA).zip', 5, '0', 'x', 'abc', 'unknown', 'now')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO manual_ids (sha1, size, system_id, title, created_at) VALUES ('abc', 5, ?1, 'Sonic (USA)', 'now')",
        [md.id],
    )
    .unwrap();

    let retroarch = crate::launch::known("retroarch").unwrap();
    let found = existing(retroarch, &home, &user, &xdg, Some(&conn));
    // SwanStation's folder needs its core info to name it: not here.
    let swan = crate::obtain::library_name(None, "swanstation").is_some();
    assert_eq!(found.saves, 3 + usize::from(swan), "{found:?}");
    let done = import(&conn, retroarch, &home, &user, &xdg).unwrap();
    assert_eq!(done.saves, found.saves);
    let md_saves = home.saves().join(md.key);
    assert_eq!(std::fs::read(md_saves.join("Sonic (USA).srm")).unwrap(), b"sonic");
    assert!(home.states().join(md.key).join("Sonic (USA).state1").is_file());
    assert!(
        home.states()
            .join(md.key)
            .join("Sonic (USA).state1.png")
            .is_file()
    );
    assert!(ra.join("saves/Mystery.srm").is_file(), "theirs stay");
    // Nothing of romforge's is replaced on a second import; undo takes them back.
    let again = import(&conn, retroarch, &home, &user, &xdg).unwrap();
    assert_eq!(again.saves, 0);
    undo(&conn, "retroarch").unwrap();
    undo(&conn, "retroarch").unwrap();
    assert!(!md_saves.join("Sonic (USA).srm").exists());
}
