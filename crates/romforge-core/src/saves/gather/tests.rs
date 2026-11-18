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
    assert!(
        gathered(&home, "duckstation"),
        "Saves/ from now on, though one stayed"
    );

    assert_eq!(undo(&conn, &home, "duckstation").unwrap(), 1);
    assert!(cards.join("Final Fantasy VII_1.mcd").exists());
    assert!(!ps1.join("Final Fantasy VII_1.mcd").exists());
    assert!(undo(&conn, &home, "duckstation").is_err(), "nothing left to undo");
    assert!(!gathered(&home, "duckstation"), "its old place again");
}

/// A gathering that fails part way puts back what it moved: never the
/// saves split between Saves/ and the old folder.
#[test]
fn a_gathering_cut_short_moves_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(&t.path().join("parent")).unwrap();
    let conn = library(&[]);
    let cards = home.emulator_config("duckstation").join("duckstation/memcards");
    put(&cards.join("A_1.mcd"), b"a");
    put(&cards.join("sub/B_1.mcd"), b"b");
    // B's place takes no files.
    let locked = home.saves().join("PlayStation/sub");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(locked.join("probe"), b"").is_ok() {
        return; // run as root: nothing is refused
    }
    let err = apply(&conn, "duckstation", &home, None).unwrap_err().to_string();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(err.contains("all left where they were"), "{err}");
    assert_eq!(std::fs::read(cards.join("A_1.mcd")).unwrap(), b"a", "A came back");
    assert!(!home.saves().join("PlayStation/A_1.mcd").exists());
    assert!(!gathered(&home, "duckstation"));
    assert!(undo(&conn, &home, "duckstation").is_err(), "nothing to undo");
}
