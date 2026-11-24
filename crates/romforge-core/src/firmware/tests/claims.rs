//! Which dropped file fills a place, and files a profile lists without a
//! hash.

use super::*;

#[test]
fn a_file_of_the_place_s_own_name_fills_it_and_known_hashes_count() {
    let t = tempfile::tempdir().unwrap();
    let home = Home::create(t.path()).unwrap();
    let dir = t.path().join("db");
    std::fs::create_dir_all(&dir).unwrap();
    let us = b"psx-us-bios".to_vec();
    let rom = b"ps1-rom".to_vec();
    // SwanStation loads ps1_rom.bin and checks no hash; DuckStation lists
    // the same file with one.
    let doc = serde_json::json!({"items": [
        {"id": "swanstation", "profile": {"cores": ["swanstation"], "systems": ["sony-playstation"],
            "files": [
                {"name": "scph5501.bin", "required": true, "sha1": [sha1(&us), sha1(b"other")]},
                {"name": "ps1_rom.bin", "size": rom.len()}
            ]}},
        {"id": "duckstation", "profile": {"cores": ["duckstation"], "systems": ["sony-playstation"],
            "files": [{"name": "PS1_ROM.BIN", "sha1": sha1(&rom)}]}}
    ]});
    std::fs::write(dir.join(FILE), doc.to_string()).unwrap();
    let drop = t.path().join("drop");
    std::fs::create_dir_all(&drop).unwrap();
    // Read first, but another file DuckStation accepts there too.
    std::fs::write(drop.join("a-other.bin"), b"other").unwrap();
    std::fs::write(drop.join("scph5501.bin"), &us).unwrap();
    std::fs::write(drop.join("ps1_rom.bin"), &rom).unwrap();

    let p = plan(&dir, &home, &Env::default(), &[drop], &mut |_, _, _| {}).unwrap();
    let by = |n: &str| p.files.iter().find(|f| f.name == n).unwrap();
    let ra = home.bios().join("RetroArch/scph5501.bin");
    let state = |n: &str| by(n).targets.iter().find(|t| t.place == ra).unwrap().state;
    assert_eq!(state("scph5501.bin"), TargetState::New);
    assert_eq!(state("a-other.bin"), TargetState::Claimed);

    let rom = by("ps1_rom.bin");
    assert_eq!(rom.matched, Match::Accepted, "a hash DuckStation knows");
    assert!(
        rom.targets
            .iter()
            .any(|t| t.place == home.bios().join("RetroArch/ps1_rom.bin"))
    );
}
