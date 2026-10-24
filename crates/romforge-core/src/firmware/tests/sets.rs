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

    let done = apply(&conn, &dir, &home, &env, &paths, &[]).unwrap();
    assert_eq!(done.written, 0, "not placed unless picked");
    let done = apply(&conn, &dir, &home, &env, &paths, std::slice::from_ref(&f.shown)).unwrap();
    assert_eq!(done.written, 1);
    assert_eq!(std::fs::read(&place).unwrap(), set);
    let r = report(&dir, &home, &env).unwrap();
    let fm7 = r.systems.iter().find(|s| s.key == "FM-7").unwrap();
    assert_eq!(fm7.emulators[0].files[0].state, State::Present);
}
