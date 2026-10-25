//! Milestone 20, step 6: a scanned file that is a BIOS file shows as one.

use super::*;

#[test]
fn a_bios_file_among_games_is_shown_as_one() {
    let t = tempfile::tempdir().unwrap();
    let us = b"psx-us-bios".to_vec();
    std::fs::write(t.path().join(FILE), list(&us)).unwrap();
    let conn = crate::db::open_in_memory().unwrap();
    assert!(index(&conn, t.path()).unwrap(), "built");
    assert!(!index(&conn, t.path()).unwrap(), "the same list: kept");

    conn.execute(
        "INSERT INTO scan_roots (root_id, path, scanned_at) VALUES (1, '/g', '2026-10-25')",
        [],
    )
    .unwrap();
    let file = |id: i64, name: &str, size: usize, sha1: &str| {
        conn.execute(
            "INSERT INTO files (file_id, root_id, path, size, crc32, md5, sha1, scanned_at)
             VALUES (?1, 1, ?2, ?3, '0', 'x', ?4, '2026-10-25')",
            rusqlite::params![id, format!("/g/{name}"), size as i64, sha1],
        )
        .unwrap();
    };
    file(1, "SCPH-5501.BIN", us.len(), &sha1(&us).to_uppercase());
    file(2, "other.bin", us.len() + 1, &sha1(&us));
    let rows = crate::view::library(&conn).unwrap().rows;
    let row = |id: &str| rows.iter().find(|r| r.id == id).unwrap();
    let bios = row("f:1");
    assert_eq!(bios.status, crate::view::RowStatus::Firmware);
    assert_eq!(bios.bios.as_deref(), Some("scph5501.bin"));
    assert_eq!(bios.system.as_deref(), Some("PlayStation"));
    assert!(
        bios.unmatched && !bios.playable,
        "no game: listed under Unidentified"
    );
    assert_eq!(
        row("f:2").status,
        crate::view::RowStatus::Unknown,
        "size and hash, never the hash alone"
    );
}
