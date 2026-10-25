use super::*;

fn put(p: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// A library holding `files` (path, system key), named by you.
fn library(files: &[(&str, &str)]) -> Connection {
    let conn = crate::db::open_in_memory().unwrap();
    conn.execute(
        "INSERT INTO scan_roots (root_id, path, scanned_at) VALUES (1, '/', 'now')",
        [],
    )
    .unwrap();
    for (i, (path, system)) in files.iter().enumerate() {
        let sha1 = format!("s{i}");
        conn.execute(
            "INSERT INTO files (file_id, root_id, path, size, crc32, md5, sha1, status, scanned_at)
             VALUES (?1, 1, ?2, 5, '0', 'x', ?3, 'unknown', 'now')",
            params![i as i64 + 1, path, sha1],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO manual_ids (sha1, size, system_id, title, created_at) VALUES (?1, 5, ?2, 'x', 'now')",
            params![sha1, systems::by_key(system).unwrap().id],
        )
        .unwrap();
    }
    conn
}

#[test]
fn memory_cards_come_in_and_go_back() {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(&t.path().join("parent")).unwrap();
    let conn = library(&[]);
    let cards = home.emulator_config("duckstation").join("duckstation/memcards");
    put(&cards.join("Final Fantasy VII_1.mcd"), b"card");
    put(&cards.join("Other_1.mcd"), b"mine");
    let ps1 = home.saves().join("PlayStation");
    put(&ps1.join("Other_1.mcd"), b"there already");

    let p = plan(&conn, "duckstation", &home, None);
    assert_eq!(p.problem, None);
    assert_eq!(p.moves.len(), 2);
    assert!(p.moves.iter().any(|m| m.taken && m.from.ends_with("Other_1.mcd")));
    assert!(
        cards.join("Final Fantasy VII_1.mcd").exists(),
        "a plan writes nothing"
    );

    let (_, n) = apply(&conn, "duckstation", &home, None).unwrap();
    assert_eq!(n, 1);
    assert_eq!(
        std::fs::read(ps1.join("Final Fantasy VII_1.mcd")).unwrap(),
        b"card"
    );
    assert_eq!(
        std::fs::read(cards.join("Other_1.mcd")).unwrap(),
        b"mine",
        "taken: stays"
    );
    assert_eq!(std::fs::read(ps1.join("Other_1.mcd")).unwrap(), b"there already");

    assert_eq!(undo(&conn, "duckstation").unwrap(), 1);
    assert!(cards.join("Final Fantasy VII_1.mcd").exists());
    assert!(!ps1.join("Final Fantasy VII_1.mcd").exists());
    assert!(undo(&conn, "duckstation").is_err(), "nothing left to undo");
}

#[test]
fn saves_beside_games_and_mednafens_go_to_their_games_system() {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(&t.path().join("parent")).unwrap();
    let games = t.path().join("games");
    let gba = games.join("Mother 3 (Japan).zip");
    let gb = games.join("Tetris (World).gb");
    let conn = library(&[
        (&gba.to_string_lossy(), "Game Boy Advance"),
        (&gb.to_string_lossy(), "Game Boy"),
    ]);
    put(&gba.with_extension("sav"), b"gba save");
    put(&gb, b"rom");

    let (_, n) = apply(&conn, "mgba", &home, None).unwrap();
    assert_eq!(n, 1);
    let to = home.saves().join("Game Boy Advance/Mother 3 (Japan).sav");
    assert_eq!(std::fs::read(&to).unwrap(), b"gba save");
    assert!(games.is_dir(), "the games' folder stays");

    // Mednafen: `<game>.<md5>.<ext>`, the game's name with dots in it.
    let sav = home.emulator_config("mednafen").join("sav");
    put(&sav.join("Nights into Dreams... (USA).0123abcd.bkr"), b"saturn");
    put(&sav.join("Unknown.4567.sav"), b"?");
    let nights = games.join("Nights into Dreams... (USA).cue");
    let conn = library(&[(&nights.to_string_lossy(), "Saturn")]);
    let p = plan(&conn, "mednafen", &home, None);
    assert_eq!(p.unplaced, [sav.join("Unknown.4567.sav")]);
    assert_eq!(
        p.moves[0].to,
        home.saves()
            .join("Saturn/Nights into Dreams... (USA).0123abcd.bkr")
    );
}

#[test]
fn dolphin_cards_keep_their_region() {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(&t.path().join("parent")).unwrap();
    let gc = home.emulator_config("dolphin").join("GC");
    put(&gc.join("USA/Card A/01-GALE-SuperSmashBros.gci"), b"gci");
    put(&gc.join("MemoryCardA.EUR.raw"), b"raw");
    put(&gc.join("USA/IPL.bin"), b"bios, not a save");
    let p = plan(&library(&[]), "dolphin", &home, None);
    let mut to: Vec<_> = p.moves.iter().map(|m| m.to.clone()).collect();
    to.sort();
    let saves = home.saves().join("GameCube");
    assert_eq!(
        to,
        [
            saves.join("MemoryCardA.EUR.raw"),
            saves.join("USA/01-GALE-SuperSmashBros.gci")
        ]
    );
    assert_eq!(plan(&library(&[]), "retroarch", &home, None).moves, []);
}
