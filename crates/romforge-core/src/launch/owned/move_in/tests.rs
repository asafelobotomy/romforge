//! Moving game data into RomForge and back, on a fake home.

use super::super::existing::Xdg;
use super::*;

fn user(t: &Path) -> UserDirs {
    let home = t.join("u");
    UserDirs {
        xdg: Xdg {
            config: home.join(".config"),
            data: home.join(".local/share"),
        },
        home,
    }
}

#[test]
fn rpcs3_s_devices_move_in_and_back_out() {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(&t.path().join("parent")).unwrap();
    let u = user(t.path());
    let conn = crate::db::open_in_memory().unwrap();
    let theirs = u.xdg.config.join("rpcs3");
    std::fs::create_dir_all(theirs.join("dev_hdd0/game/BLUS00001")).unwrap();
    std::fs::write(theirs.join("dev_hdd0/game/BLUS00001/EBOOT.BIN"), b"eboot").unwrap();
    std::fs::write(theirs.join("dev_hdd0/save.dat"), b"save").unwrap();

    let p = plan("rpcs3", &home, Some(&u));
    assert_eq!(p.problem, None, "{p:?}");
    assert_eq!((p.items.len(), p.items[0].files, p.bytes), (1, 2, 9));
    let ours = home.emulator_config("rpcs3").join("rpcs3/dev_hdd0");
    assert_eq!(p.items[0].to, ours);

    apply(&conn, "rpcs3", &home, Some(&u)).unwrap();
    assert_eq!(
        std::fs::read(ours.join("game/BLUS00001/EBOOT.BIN")).unwrap(),
        b"eboot"
    );
    assert!(
        !theirs.join("dev_hdd0").exists(),
        "the original went to the trash"
    );
    assert!(theirs::moved_in(&home.emulator_config("rpcs3")));
    // RPCS3's config now points at RomForge's copy.
    let dir = home.emulator_config("rpcs3");
    let prep = super::super::gamedata::gamedata("rpcs3", &dir, &home.bios().join("rpcs3"), Some(&u)).unwrap();
    let vfs = &prep.writes[0].keys;
    assert!(
        vfs.iter()
            .any(|(k, v)| k == "/dev_hdd0/" && v.contains(&ours.display().to_string())),
        "{vfs:?}"
    );
    let trash = crate::cleanup::trash_contents(&conn, None).unwrap();
    assert_eq!(trash.batches.iter().map(|b| b.kind).collect::<Vec<_>>(), ["move"]);
    assert!(
        plan("rpcs3", &home, Some(&u))
            .problem
            .unwrap()
            .contains("already")
    );

    // RPCS3 saves into RomForge's copy; undo keeps that in the trash.
    std::fs::write(ours.join("save.dat"), b"save, played on").unwrap();
    undo(&conn, "rpcs3", &home).unwrap();
    assert_eq!(std::fs::read(theirs.join("dev_hdd0/save.dat")).unwrap(), b"save");
    assert!(!ours.exists() && !theirs::moved_in(&dir));
    let kept = home
        .trash()
        .join("move-1-undone")
        .join(ours.strip_prefix("/").unwrap());
    assert_eq!(std::fs::read(kept.join("save.dat")).unwrap(), b"save, played on");
    assert!(
        crate::cleanup::trash_contents(&conn, None)
            .unwrap()
            .batches
            .is_empty()
    );
}

#[test]
fn nothing_moves_over_files_or_without_data() {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(&t.path().join("parent")).unwrap();
    let u = user(t.path());
    let conn = crate::db::open_in_memory().unwrap();
    assert!(
        plan("cemu", &home, Some(&u))
            .problem
            .unwrap()
            .contains("none of its data")
    );
    assert!(plan("dolphin", &home, Some(&u)).problem.is_some());
    // A 3DS NAND, but RomForge's place holds files already.
    let nand = u.xdg.data.join("citra-emu/nand");
    std::fs::create_dir_all(&nand).unwrap();
    std::fs::write(nand.join("data.bin"), b"nand").unwrap();
    let saves = home.saves().join("Nintendo 3DS/nand");
    std::fs::create_dir_all(&saves).unwrap();
    std::fs::write(saves.join("other.bin"), b"x").unwrap();
    let p = plan("citra", &home, Some(&u));
    assert!(p.problem.unwrap().contains("holds files already"));
    assert!(apply(&conn, "citra", &home, Some(&u)).is_err());
    assert!(nand.join("data.bin").exists(), "untouched");
    // Emptied: then it moves, and Azahar's config uses RomForge's copy.
    std::fs::remove_file(saves.join("other.bin")).unwrap();
    apply(&conn, "citra", &home, Some(&u)).unwrap();
    assert_eq!(std::fs::read(saves.join("data.bin")).unwrap(), b"nand");
    assert!(!nand.exists());
}

/// An original that can't go to the trash: the emulator already uses the
/// whole copies (never a config pointing at a half-emptied folder), the
/// original stays, and undo puts everything back.
#[test]
fn a_move_cut_short_never_strands_the_data() {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(&t.path().join("parent")).unwrap();
    let u = user(t.path());
    let conn = crate::db::open_in_memory().unwrap();
    let theirs = u.xdg.config.join("rpcs3");
    for dev in ["dev_hdd0", "dev_hdd1"] {
        std::fs::create_dir_all(theirs.join(dev)).unwrap();
        std::fs::write(theirs.join(dev).join("save.dat"), dev).unwrap();
    }
    assert_eq!(plan("rpcs3", &home, Some(&u)).items.len(), 2);
    // Something already where dev_hdd1 would go in the trash.
    let blocked = home
        .trash()
        .join("move-1")
        .join(theirs.join("dev_hdd1").strip_prefix("/").unwrap());
    std::fs::create_dir_all(blocked.parent().unwrap()).unwrap();
    std::fs::write(&blocked, b"in the way").unwrap();

    let err = apply(&conn, "rpcs3", &home, Some(&u)).unwrap_err().to_string();
    assert!(err.contains("couldn't go to the trash"), "{err}");
    let dir = home.emulator_config("rpcs3");
    assert!(theirs::moved_in(&dir), "the copies are in use");
    assert_eq!(
        std::fs::read(theirs.join("dev_hdd1/save.dat")).unwrap(),
        b"dev_hdd1"
    );
    for dev in ["dev_hdd0", "dev_hdd1"] {
        assert!(dir.join("rpcs3").join(dev).join("save.dat").is_file(), "{dev}");
    }

    undo(&conn, "rpcs3", &home).unwrap();
    assert!(!theirs::moved_in(&dir));
    for dev in ["dev_hdd0", "dev_hdd1"] {
        assert_eq!(
            std::fs::read(theirs.join(dev).join("save.dat")).unwrap(),
            dev.as_bytes()
        );
        assert!(!dir.join("rpcs3").join(dev).exists(), "{dev}");
    }
}
