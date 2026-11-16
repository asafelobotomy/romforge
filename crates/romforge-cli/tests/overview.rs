//! `overview --json` and `games --json`, the reads uQ Settings' romforge page runs.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

fn run(db: &Path, args: &[&str]) -> String {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let out = Command::new(env!("CARGO_BIN_EXE_romforge-cli"))
        .current_dir(&fixtures)
        .env("ROMFORGE_DB", db)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

fn json(db: &Path, args: &[&str]) -> Value {
    serde_json::from_str(&run(db, args)).unwrap()
}

fn library(db: &Path) {
    let dats: Vec<String> = ["gb", "nes", "snes"]
        .iter()
        .map(|d| format!("dats/{d}.dat"))
        .collect();
    let mut import = vec!["dat", "import"];
    import.extend(dats.iter().map(String::as_str));
    run(db, &import);
    run(db, &["scan", ".", "-q"]);
}

#[test]
fn overview_counts_what_the_scan_found() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("romforge.db");
    // An empty library is an answer, not an error.
    let empty = json(&db, &["overview", "--json"]);
    assert_eq!(empty["schema_version"], 1);
    assert_eq!(empty["totals"]["games"], 0);
    assert_eq!(empty["recent"].as_array().unwrap().len(), 0);

    library(&db);
    let o = json(&db, &["overview", "--json"]);
    assert_eq!(o["totals"]["verified"], o["totals"]["games"]);
    assert!(o["totals"]["games"].as_u64().unwrap() >= 6);
    assert_eq!(o["lists"]["dats"], 3);
    let keys: Vec<&str> = o["platforms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["system"].as_str().unwrap())
        .collect();
    assert!(keys.contains(&"NES") && keys.contains(&"Game Boy"), "{keys:?}");
    // Games of systems with no DAT here are not counted, but their files are scanned.
    assert!(o["recent"][0]["added"].as_str().is_some());
    assert!(o["recent"].as_array().unwrap().len() <= 12);
    // The text form says the same.
    assert!(run(&db, &["overview"]).contains("verified"));
}

#[test]
fn games_filter_sort_and_page() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("romforge.db");
    library(&db);

    let nes = json(&db, &["games", "--json", "--platform", "nes"]);
    assert_eq!(nes["total"], 2);
    assert!(
        nes["games"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g["system"] == "NES")
    );

    let found = json(&db, &["games", "--json", "--search", "NES 2.0"]);
    assert_eq!(found["total"], 1);
    assert_eq!(found["games"][0]["title"], "Test NES 2.0");
    let none = json(&db, &["games", "--json", "--search", "zzz"]);
    assert_eq!(none["total"], 0);

    let page = json(&db, &["games", "--json", "--limit", "1", "--offset", "1"]);
    assert_eq!(page["games"].as_array().unwrap().len(), 1);
    assert!(page["total"].as_u64().unwrap() > 1);

    let unidentified = json(&db, &["games", "--json", "--status", "unidentified"]);
    assert!(
        unidentified["games"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g["status"] != "patch")
    );
    let verified = json(
        &db,
        &[
            "games", "--json", "--status", "verified", "--sort", "title", "--desc",
        ],
    );
    let titles: Vec<&str> = verified["games"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["title"].as_str().unwrap())
        .collect();
    let mut sorted = titles.clone();
    sorted.sort_by_key(|t| t.to_lowercase());
    sorted.reverse();
    assert_eq!(titles, sorted);
}

#[test]
fn browse_takes_its_choices_as_positional_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("romforge.db");
    library(&db);
    let all = json(&db, &["browse", "all", "all", "title", "50", "0"]);
    assert_eq!(all, {
        let mut v = json(&db, &["games", "--json"]);
        v["total"] = all["total"].clone();
        v
    });
    let nes = json(&db, &["browse", "NES", "verified", "year:desc", "1", "0"]);
    assert_eq!(nes["total"], 2);
    assert_eq!(nes["games"].as_array().unwrap().len(), 1);
    let any = json(&db, &["browse", "all", "all", "title", "50", "0", "*"]);
    assert_eq!(any["total"], all["total"]);
    let found = json(&db, &["browse", "all", "all", "title", "50", "0", "NES 2.0"]);
    assert_eq!(found["games"][0]["title"], "Test NES 2.0");
    let out = Command::new(env!("CARGO_BIN_EXE_romforge-cli"))
        .env("ROMFORGE_DB", &db)
        .args(["browse", "all", "all", "title:sideways", "5", "0"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn a_bad_filter_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("romforge.db");
    let out = Command::new(env!("CARGO_BIN_EXE_romforge-cli"))
        .env("ROMFORGE_DB", &db)
        .args(["games", "--json", "--status", "nope"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--status takes"));
}
