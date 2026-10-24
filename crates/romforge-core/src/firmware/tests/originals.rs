//! Settings: not leaving BIOS files where they are. An import's originals
//! go to the RomForge trash once their copies are in Bios/; undo puts them
//! back; Empty trash deletes them, and the import can't be undone after.

use super::*;

#[test]
fn moved_originals_come_back_with_undo_and_go_with_the_trash() {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(t.path()).unwrap();
    let dir = t.path().join("db");
    std::fs::create_dir_all(&dir).unwrap();
    let us = b"psx-us-bios".to_vec();
    std::fs::write(dir.join(FILE), list(&us)).unwrap();
    let (env, conn) = (Env::default(), crate::db::open_in_memory().unwrap());
    let found = t.path().join("Emulation/bios");
    std::fs::create_dir_all(&found).unwrap();
    std::fs::write(found.join("scph5501.bin"), &us).unwrap();
    std::fs::write(found.join("notes.txt"), b"mine").unwrap();
    let paths = vec![found.clone()];

    let done = apply(&conn, &dir, &home, &env, &paths, &[], true).unwrap();
    assert!(done.written >= 1 && done.moved == 1, "{done:?}");
    assert!(!found.join("scph5501.bin").exists(), "moved to the trash");
    assert!(found.join("notes.txt").exists(), "what isn't a BIOS file stays");
    assert_eq!(
        std::fs::read(home.bios().join("RetroArch/scph5501.bin")).unwrap(),
        us
    );
    let trash = crate::cleanup::trash_contents(&conn, None).unwrap();
    assert_eq!(trash.batches.iter().map(|b| b.kind).collect::<Vec<_>>(), ["bios"]);

    undo(&conn).unwrap();
    assert_eq!(std::fs::read(found.join("scph5501.bin")).unwrap(), us, "back");
    assert!(!home.bios().join("RetroArch/scph5501.bin").exists());
    assert!(
        crate::cleanup::trash_contents(&conn, None)
            .unwrap()
            .batches
            .is_empty()
    );

    // Moved again, then the trash emptied: gone for good, no undo.
    apply(&conn, &dir, &home, &env, &paths, &[], true).unwrap();
    let emptied = crate::cleanup::empty_trash(&conn, None).unwrap();
    assert_eq!(emptied.files, 1);
    assert!(!found.join("scph5501.bin").exists());
    assert!(undo(&conn).is_err(), "its originals are gone");
    // Left where they are (the default): copied, nothing moves.
    std::fs::write(found.join("scph5501.bin"), &us).unwrap();
    std::fs::remove_dir_all(home.bios()).unwrap();
    let done = apply(&conn, &dir, &home, &env, &paths, &[], false).unwrap();
    assert_eq!(done.moved, 0);
    assert!(found.join("scph5501.bin").exists());
}
