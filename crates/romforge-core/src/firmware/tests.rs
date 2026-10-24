use super::*;

fn sha1(data: &[u8]) -> String {
    crate::hash::hash_stream(data, data.len() as u64)
        .unwrap()
        .raw
        .sha1
}

/// A hand-made list in RetroBIOS's shape: SwanStation (a PlayStation core)
/// with regional BIOS files, DuckStation, and an alias.
fn list(us: &[u8]) -> String {
    serde_json::json!({
        "generated_at": "2026-10-19T00:00:00Z",
        "items": [
            {"id": "swanstation", "profile": {
                "display_name": "Sony - PlayStation (SwanStation)",
                "cores": ["swanstation"], "systems": ["sony-playstation"],
                "files": [
                    {"name": "scph5500.bin", "required": true, "region": ["japan"], "size": 6,
                     "md5": "00000000000000000000000000000000"},
                    {"name": "scph5501.bin", "required": true, "region": ["north-america"], "size": us.len(),
                     "sha1": [sha1(us), "1111111111111111111111111111111111111111"]},
                    {"name": "ps1_rom.bin", "required": false, "path": "extra/ps1_rom.bin"},
                    {"name": "wrong.bin", "required": false, "md5": "22222222222222222222222222222222,33333333333333333333333333333333"},
                    {"name": "saturn.bin", "required": true, "system": "sega-saturn"}
                ]}},
            {"id": "duckstation", "profile": {
                "display_name": "DuckStation", "cores": ["duckstation"], "systems": ["sony-playstation"],
                "files": [{"name": "scph5501.bin", "required": true, "sha1": sha1(us)}]}},
            {"id": "swan_alias", "profile": {"alias_of": "swanstation", "cores": ["swan_alias_core"]}}
        ]
    })
    .to_string()
}

#[test]
fn the_list_is_read_leniently() {
    let (profiles, generated) = data::parse(list(b"us").as_bytes()).unwrap();
    assert_eq!(generated, "2026-10-19T00:00:00Z");
    assert_eq!(profiles.len(), 2, "the alias is folded into its target");
    let swan = &profiles[0];
    assert_eq!(swan.cores, ["swanstation", "swan_alias", "swan_alias_core"]);
    assert_eq!(swan.files[1].sha1.len(), 2);
    assert_eq!(swan.files[3].md5.len(), 2, "comma-separated");
    assert_eq!(swan.files[0].regions, ["japan"]);
    assert_eq!(swan.files[2].place(), "extra/ps1_rom.bin");
    assert!(data::parse(b"{}").is_err());
}

#[test]
fn a_path_that_leaves_the_bios_folder_is_dropped() {
    let doc = serde_json::json!({"items": [{"id": "x", "profile": {"files": [
        {"name": "ok.bin", "path": "sub/ok.bin", "standalone_path": "GC/USA/IPL.bin"},
        {"name": "../escape.bin"},
        {"name": "/etc/passwd"},
        {"name": "a.bin", "path": "../../a.bin"},
        {"name": "b.bin", "path": "/abs/b.bin"},
        {"name": "c.bin", "standalone_path": "x/../../c.bin"},
        {"name": "d.bin", "path": "..\\d.bin"},
        {"name": "e.bin", "path": "./e.bin"},
        {"name": ""}
    ]}}]})
    .to_string();
    let (profiles, _) = data::parse(doc.as_bytes()).unwrap();
    let names: Vec<&str> = profiles[0].files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["ok.bin"]);
}

#[test]
fn bios_files_are_checked_where_each_emulator_finds_them() {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(t.path()).unwrap();
    let dir = t.path().join("db");
    std::fs::create_dir_all(&dir).unwrap();
    let us = b"psx-us-bios".to_vec();
    std::fs::write(dir.join(FILE), list(&us)).unwrap();

    let empty = Env::default();
    let ra = home.bios().join("RetroArch");
    std::fs::create_dir_all(ra.join("extra")).unwrap();
    std::fs::write(ra.join("scph5501.bin"), &us).unwrap();
    std::fs::write(ra.join("extra/ps1_rom.bin"), b"anything").unwrap();
    std::fs::write(ra.join("wrong.bin"), b"not it").unwrap();

    let r = report(&dir, &home, &empty).unwrap();
    assert!(
        r.systems.iter().all(|s| s.key != "Arcade"),
        "arcade BIOS sets are MAME's DATs'"
    );
    let ps = r.systems.iter().find(|s| s.key == "PlayStation").unwrap();
    let swan = ps
        .emulators
        .iter()
        .find(|e| e.name == "retroarch:swanstation")
        .unwrap();
    assert_eq!(swan.label, "RetroArch: Sony - PlayStation (SwanStation)");
    assert!(!swan.installed);
    let state = |n: &str| swan.files.iter().find(|f| f.name == n).map(|f| f.state);
    assert_eq!(state("scph5501.bin"), Some(State::Accepted));
    assert_eq!(state("scph5500.bin"), Some(State::Missing));
    assert_eq!(
        state("ps1_rom.bin"),
        Some(State::Present),
        "no hash to check it by"
    );
    assert_eq!(state("wrong.bin"), Some(State::WrongHash));
    assert_eq!(state("saturn.bin"), None, "another system's file");
    assert!(
        swan.ready,
        "one region's BIOS is enough to start that region's games"
    );
    assert_eq!(swan.regions, ["north-america"]);

    // DuckStation looks in its own folder, where nothing is yet.
    let duck = ps.emulators.iter().find(|e| e.name == "duckstation").unwrap();
    assert_eq!(duck.files[0].place, home.bios().join("duckstation/scph5501.bin"));
    assert!(!duck.ready);
    // A core RetroBIOS has no profile for.
    let other = ps
        .emulators
        .iter()
        .find(|e| e.name == "retroarch:pcsx_rearmed")
        .unwrap();
    assert!(!other.profiled && other.files.is_empty() && other.ready);

    std::fs::remove_file(dir.join(FILE)).unwrap();
    assert!(
        report(&dir, &home, &empty)
            .unwrap_err()
            .to_string()
            .contains("firmware download")
    );
}

fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in entries {
        w.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut w, data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

#[test]
fn a_drop_is_matched_by_contents_and_placed_once() {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(t.path()).unwrap();
    let dir = t.path().join("db");
    std::fs::create_dir_all(&dir).unwrap();
    let us = b"psx-us-bios".to_vec();
    std::fs::write(dir.join(FILE), list(&us)).unwrap();
    let env = Env::default();
    let conn = crate::db::open_in_memory().unwrap();

    // A pack that renamed the BIOS, the same file again in a zip inside a
    // zip, a file whose name fits but has no hash listed, and a stranger.
    let drop = t.path().join("drop");
    std::fs::create_dir_all(&drop).unwrap();
    std::fs::write(drop.join("renamed.bin"), &us).unwrap();
    let inner = zip_of(&[("bios/again.bin", &us)]);
    std::fs::write(
        drop.join("pack.zip"),
        zip_of(&[("inner.zip", &inner), ("ps1_rom.bin", b"rom")]),
    )
    .unwrap();
    std::fs::write(drop.join("notes.txt"), b"hello").unwrap();
    std::fs::write(drop.join("big.zip.001"), b"slice").unwrap();
    let paths = vec![drop.clone()];

    let p = plan(&dir, &home, &env, &paths).unwrap();
    let by = |n: &str| p.files.iter().find(|f| f.name == n).unwrap();
    let renamed = by("renamed.bin");
    assert_eq!(renamed.matched, Match::Accepted);
    let places: Vec<&Path> = renamed.targets.iter().map(|t| t.place.as_path()).collect();
    let (ra, duck) = (
        home.bios().join("RetroArch/scph5501.bin"),
        home.bios().join("duckstation/scph5501.bin"),
    );
    assert_eq!(places, [ra.as_path(), duck.as_path()]);
    // The copy in the nested zip is read first (`pack.zip` sorts before
    // `renamed.bin`), so it is the one written; this one is a duplicate.
    assert!(renamed.targets.iter().all(|t| t.state == TargetState::Claimed));
    assert!(
        renamed.targets[0]
            .used_by
            .contains(&"RetroArch: Sony - PlayStation (SwanStation)".to_owned())
    );
    let again = by("again.bin");
    assert!(
        again.shown.ends_with("pack.zip › inner.zip › bios/again.bin"),
        "{}",
        again.shown
    );
    assert!(again.targets.iter().all(|t| t.state == TargetState::New));
    assert_eq!(by("ps1_rom.bin").matched, Match::NameOnly);
    assert_eq!(by("notes.txt").matched, Match::Unknown);
    assert!(
        p.skipped
            .iter()
            .any(|(what, why)| what.ends_with("big.zip.001") && why.contains("split"))
    );
    assert_eq!(p.new_places, 2);
    assert!(!ra.exists(), "planning writes nothing");

    // Apply, with the name-only file picked too.
    let ps1 = by("ps1_rom.bin").shown.clone();
    let done = apply(&conn, &dir, &home, &env, &paths, &[ps1]).unwrap();
    assert_eq!((done.written, done.linked), (2, 1));
    assert_eq!(std::fs::read(&ra).unwrap(), us);
    assert_eq!(std::fs::read(&duck).unwrap(), us);
    assert_eq!(
        std::fs::read(home.bios().join("RetroArch/extra/ps1_rom.bin")).unwrap(),
        b"rom"
    );
    assert!(drop.join("renamed.bin").exists(), "copied, never moved");
    let r = report(&dir, &home, &env).unwrap();
    let ps = r.systems.iter().find(|s| s.key == "PlayStation").unwrap();
    assert!(
        ps.emulators
            .iter()
            .find(|e| e.name == "duckstation")
            .unwrap()
            .ready
    );

    // Again: everything is there already.
    let p = plan(&dir, &home, &env, &paths).unwrap();
    assert_eq!(p.new_places, 0);
    assert!(
        p.files
            .iter()
            .find(|f| f.name == "renamed.bin")
            .unwrap()
            .targets
            .iter()
            .all(|t| t.state == TargetState::AlreadyThere)
    );

    // Undo takes the files away; another file at a place is never replaced.
    assert_eq!(undo(&conn).unwrap(), 3);
    assert!(!ra.exists() && !duck.exists());
    std::fs::write(&duck, b"someone else's").unwrap();
    let p = plan(&dir, &home, &env, &paths).unwrap();
    let states: Vec<TargetState> = p
        .files
        .iter()
        .find(|f| f.name == "again.bin")
        .unwrap()
        .targets
        .iter()
        .map(|t| t.state)
        .collect();
    assert_eq!(states, [TargetState::New, TargetState::Taken]);
    apply(&conn, &dir, &home, &env, &paths, &[]).unwrap();
    assert_eq!(std::fs::read(&duck).unwrap(), b"someone else's");
    assert!(
        undo(&conn).is_ok() && undo(&conn).is_err(),
        "nothing left to undo"
    );
}

/// Cores listed as an object or with `_libretro`, MAME's machines (their
/// own sets only), and files that don't decide readiness: a sub-system's,
/// a coprocessor chip's.
#[test]
fn each_emulator_gets_its_own_files_and_only_those_gate_it() {
    let doc = serde_json::json!({"items": [
        {"id": "bsnes", "profile": {"cores": {"bsnes": null}, "systems": ["nintendo-snes", "nintendo-satellaview"],
            "files": [
                {"name": "dsp3.program.rom", "required": true, "system": "nintendo-snes"},
                {"name": "BS-X.bin", "required": true, "system": "nintendo-satellaview"}]}},
        {"id": "snes9x", "profile": {"cores": ["snes9x"], "systems": ["snes", "sufami-turbo"],
            "files": [
                {"name": "own.bin", "required": true, "system": "nintendo-snes"},
                {"name": "STBIOS.bin", "required": true, "system": "sufami-turbo"}]}},
        {"id": "o2em", "profile": {"cores": ["o2em_libretro"], "systems": ["odyssey2"],
            "files": [{"name": "o2rom.bin", "required": true}]}},
        {"id": "mame", "profile": {"cores": ["mame"], "systems": ["fm7", "snk-neogeo-mvs"],
            "files": [
                {"name": "fm7.zip", "required": true, "system": "fm7"},
                {"name": "neogeo.zip", "required": true}]}}
    ]})
    .to_string();
    let (profiles, _) = data::parse(doc.as_bytes()).unwrap();
    assert_eq!(profiles[0].cores, ["bsnes"], "an object's keys");
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(t.path()).unwrap();
    let dir = t.path().join("db");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(FILE), doc).unwrap();
    let r = report(&dir, &home, &Env::default()).unwrap();
    let emu = |sys: &str, name: &str| {
        r.systems
            .iter()
            .find(|s| s.key == sys)
            .and_then(|s| s.emulators.iter().find(|e| e.name == name))
            .cloned()
            .unwrap()
    };
    let bsnes = emu("SNES", "retroarch:bsnes");
    assert!(bsnes.ready, "neither the chip nor Satellaview's BIOS decides it");
    let only: Vec<_> = bsnes.files.iter().map(|f| f.only_for.as_deref()).collect();
    assert_eq!(only, [Some("games with that chip"), Some("Satellaview games")]);
    // Its files tied to another spelling of the system are its own.
    let snes9x = emu("SNES", "retroarch:snes9x");
    let only: Vec<_> = snes9x.files.iter().map(|f| f.only_for.as_deref()).collect();
    assert_eq!(only, [None, Some("Sufami Turbo games")]);
    assert!(!snes9x.ready, "the console's own BIOS decides it");
    let o2 = emu("Odyssey2", "retroarch:o2em");
    assert!(o2.profiled && !o2.ready, "o2em_libretro is the o2em core");
    let fm7 = emu("FM-7", "mame-fm7");
    let names: Vec<_> = fm7.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["fm7.zip"], "not another machine's set");
    assert_eq!(fm7.files[0].place, home.bios().join("mame/fm7.zip"));
    let towns = emu("FM Towns", "mame-fmtowns");
    let names: Vec<_> = towns
        .files
        .iter()
        .map(|f| (f.name.as_str(), f.required))
        .collect();
    assert_eq!(
        names,
        [("fmtowns.zip", true)],
        "its set, though the list has none"
    );
    assert!(!towns.ready);
}

mod sets;
