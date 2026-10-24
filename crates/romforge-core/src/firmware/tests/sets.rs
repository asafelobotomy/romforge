//! A MAME machine's set dropped as it is: offered whole (by its name, as
//! no list can give a zip's hash), placed only when picked.

use super::*;

#[test]
fn a_dropped_machine_set_is_offered_whole() {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(t.path()).unwrap();
    let dir = t.path().join("db");
    std::fs::create_dir_all(&dir).unwrap();
    let doc = serde_json::json!({"items": [
        {"id": "mame", "profile": {"cores": ["mame"], "systems": ["fm7"], "files": []}}
    ]});
    std::fs::write(dir.join(FILE), doc.to_string()).unwrap();
    let env = Env::default();
    let conn = crate::db::open_in_memory().unwrap();
    let drop = t.path().join("drop");
    std::fs::create_dir_all(&drop).unwrap();
    let set = zip_of(&[("fm7.rom", b"fm7")]);
    std::fs::write(drop.join("FM7.zip"), &set).unwrap();
    std::fs::write(drop.join("other.zip"), zip_of(&[("x.rom", b"x")])).unwrap();
    let paths = vec![drop.clone()];

    let p = plan(&dir, &home, &env, &paths).unwrap();
    let whole: Vec<_> = p.files.iter().filter(|f| f.source.members.is_empty()).collect();
    assert_eq!(whole.len(), 1, "only the set is offered whole: {:?}", p.files);
    let f = whole[0];
    assert_eq!(f.matched, Match::NameOnly);
    let place = home.bios().join("mame/fm7.zip");
    assert_eq!(f.targets.iter().map(|t| &t.place).collect::<Vec<_>>(), [&place]);
    assert_eq!(f.targets[0].state, TargetState::New);
    assert!(
        p.files.iter().any(|f| f.source.members == ["fm7.rom"]),
        "its members too"
    );

    let done = apply(&conn, &dir, &home, &env, &paths, &[], false).unwrap();
    assert_eq!(done.written, 0, "not placed unless picked");
    let done = apply(
        &conn,
        &dir,
        &home,
        &env,
        &paths,
        std::slice::from_ref(&f.shown),
        false,
    )
    .unwrap();
    assert_eq!(done.written, 1);
    assert_eq!(std::fs::read(&place).unwrap(), set);
    let r = report(&conn, &dir, &home, &env).unwrap();
    let fm7 = r.systems.iter().find(|s| s.key == "FM-7").unwrap();
    assert_eq!(
        fm7.emulators[0].files[0].state,
        State::PlacedByYou,
        "yours, never accepted"
    );
    undo(&conn).unwrap();
}

/// A BIOS zip the list gives members for: checked by what it holds, and a
/// dropped zip holding them all is accepted and placed whole, whatever its
/// name.
#[test]
fn a_bios_zip_is_checked_and_matched_by_its_members() {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(t.path()).unwrap();
    let dir = t.path().join("db");
    std::fs::create_dir_all(&dir).unwrap();
    let member =
        |name: &str, data: &[u8]| serde_json::json!({"name": name, "size": data.len(), "sha1": sha1(data)});
    let doc = serde_json::json!({"items": [
        {"id": "mame", "profile": {"cores": ["mame"], "systems": ["fm7"], "files": [
            {"name": "fm7.zip", "system": "fm7", "required": true,
             "contents": [member("fm7.rom", b"fm7 rom"), member("boot.rom", b"boot rom")]}
        ]}}
    ]});
    std::fs::write(dir.join(FILE), doc.to_string()).unwrap();
    let (env, conn) = (Env::default(), crate::db::open_in_memory().unwrap());
    let place = home.bios().join("mame/fm7.zip");
    let state = || {
        let r = report(&conn, &dir, &home, &env).unwrap();
        let f = r.systems.iter().find(|s| s.key == "FM-7").unwrap().emulators[0].files[0].clone();
        (f.state, f.missing)
    };
    assert_eq!(state().0, State::Missing);
    // One member of two, renamed: incomplete, and which one is missing.
    std::fs::create_dir_all(place.parent().unwrap()).unwrap();
    std::fs::write(&place, zip_of(&[("renamed.bin", b"fm7 rom")])).unwrap();
    assert_eq!(state(), (State::Incomplete, vec!["boot.rom".to_owned()]));
    std::fs::remove_file(&place).unwrap();

    // Dropped under another name, holding both (and an extra): placed whole.
    let drop = t.path().join("drop");
    std::fs::create_dir_all(&drop).unwrap();
    let zip = zip_of(&[
        ("a.bin", b"fm7 rom"),
        ("b.bin", b"boot rom"),
        ("readme.txt", b"hi"),
    ]);
    std::fs::write(drop.join("my set.zip"), &zip).unwrap();
    let paths = vec![drop.clone()];
    let p = plan(&dir, &home, &env, &paths).unwrap();
    let whole = p
        .files
        .iter()
        .find(|f| f.source.members.is_empty())
        .expect("the zip itself");
    assert_eq!(whole.matched, Match::Accepted);
    assert_eq!(whole.targets[0].place, place);
    let done = apply(&conn, &dir, &home, &env, &paths, &[], false).unwrap();
    assert_eq!(done.written, 1);
    assert_eq!(std::fs::read(&place).unwrap(), zip, "copied as it is");
    assert_eq!(state(), (State::Accepted, Vec::new()));
}
