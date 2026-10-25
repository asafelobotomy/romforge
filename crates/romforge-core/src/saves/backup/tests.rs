use super::*;

fn put(p: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

#[test]
fn times_name_the_copies() {
    assert_eq!(stamp(0), "1970-01-01T00-00-00Z");
    assert_eq!(stamp(1_761_404_400), "2025-10-25T15-00-00Z");
    assert_eq!(stamp(951_782_400), "2000-02-29T00-00-00Z");
}

#[test]
fn a_changed_save_gets_a_copy_and_old_ones_go() {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(&t.path().join("parent")).unwrap();
    let wii = home.emulator_config("dolphin").join("Wii");
    let save = Path::new("title/00010000/524d4745/data");
    put(&wii.join(save).join("banner.bin"), b"banner");
    put(&wii.join(save).join("rmg/save.dat"), b"one");
    put(
        &wii.join("title/00010000/48414141/content/00000000.app"),
        b"a channel, no save",
    );

    let done = run(&home, "dolphin", None, 100).unwrap();
    assert_eq!((done.copied, done.same), (1, 0));
    let copies = home.saves().join("Wii/backup/dolphin").join(save);
    assert_eq!(
        std::fs::read(copies.join(stamp(100)).join("rmg/save.dat")).unwrap(),
        b"one"
    );

    // Unchanged: no new copy. Changed: one; past KEEP, the oldest go.
    assert_eq!(run(&home, "dolphin", None, 200).unwrap().same, 1);
    for (i, now) in (300..300 + KEEP as u64 + 1).enumerate() {
        put(&wii.join(save).join("rmg/save.dat"), format!("v{i}").as_bytes());
        run(&home, "dolphin", None, now).unwrap();
    }
    let v = versions(&copies);
    assert_eq!(v.len(), KEEP);
    assert_eq!(v[0], stamp(300 + KEEP as u64));
    assert!(!copies.join(stamp(100)).exists());

    let listed = list(&home, "dolphin", None);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].save, save);
    assert_eq!(listed[0].versions, v);
    assert!(run(&home, "duckstation", None, 1).unwrap() == BackedUp::default());
}

#[test]
fn a_save_comes_back_from_its_copy_and_the_restore_undoes() {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(&t.path().join("parent")).unwrap();
    let conn = crate::db::open_in_memory().unwrap();
    let wii = home.emulator_config("dolphin").join("Wii");
    let save = Path::new("title/00010000/524d4745/data");
    put(&wii.join(save).join("save.dat"), b"good");
    run(&home, "dolphin", None, 100).unwrap();
    put(&wii.join(save).join("save.dat"), b"broken");
    put(&wii.join(save).join("extra.dat"), b"new file");

    let p = restore::plan(&home, "dolphin", None, save, None);
    assert_eq!(p.problem, None);
    assert_eq!(p.version, stamp(100));
    assert!(p.replaces);
    assert!(
        restore::plan(&home, "dolphin", None, Path::new("../x"), None)
            .problem
            .is_some()
    );
    assert!(
        restore::plan(&home, "dolphin", None, save, Some("1999-01-01T00-00-00Z"))
            .problem
            .is_some()
    );

    restore::apply(&conn, &home, "dolphin", None, save, None).unwrap();
    assert_eq!(std::fs::read(wii.join(save).join("save.dat")).unwrap(), b"good");
    assert!(!wii.join(save).join("extra.dat").exists());

    let back = restore::undo(&conn, &home, "dolphin").unwrap();
    assert_eq!(back, wii.join(save));
    assert_eq!(std::fs::read(wii.join(save).join("save.dat")).unwrap(), b"broken");
    assert_eq!(
        std::fs::read(wii.join(save).join("extra.dat")).unwrap(),
        b"new file"
    );
    assert!(restore::undo(&conn, &home, "dolphin").is_err());
}
