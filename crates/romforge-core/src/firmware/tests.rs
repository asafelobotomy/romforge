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
