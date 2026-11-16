//! `oysterav`: oysterAV looks at what romforge keeps. A fake `oyst-cli` stands in for the real one.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

fn fake(dir: &Path, body: &str) -> std::path::PathBuf {
    let p = dir.join("fake-oyst-cli");
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

fn run(db: &Path, cli: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_romforge-cli"))
        .env("ROMFORGE_DB", db)
        .env("ROMFORGE_OYST_CLI", cli)
        .args(args)
        .output()
        .unwrap()
}

fn events(out: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn a_clean_place_is_reported_clean_with_exit_0() {
    let d = tempfile::tempdir().unwrap();
    let games = d.path().join("games");
    std::fs::create_dir(&games).unwrap();
    let cli = fake(
        d.path(),
        r#"echo '{"event":"result","clean":true,"findings":[],"pack_errors":[]}'"#,
    );
    let out = run(
        &d.path().join("r.db"),
        &cli,
        &["oysterav", "--ndjson", games.to_str().unwrap()],
    );
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let ev = events(&out);
    assert_eq!(ev[0]["event"], "start");
    assert_eq!(ev[1]["event"], "scanned");
    assert_eq!(ev[1]["state"], "clean");
    assert_eq!(ev[2]["event"], "done");
    assert_eq!(ev[2]["clean"], true);
}

#[test]
fn a_threat_names_the_file_and_exits_1_without_touching_it() {
    let d = tempfile::tempdir().unwrap();
    let games = d.path().join("games");
    std::fs::create_dir(&games).unwrap();
    let rom = games.join("bad.zip");
    std::fs::write(&rom, "x").unwrap();
    let cli = fake(
        d.path(),
        &format!(
            r#"echo '{{"event":"result","clean":false,"findings":[{{"path":"{}","threat_name":"Bad.One"}}],"pack_errors":[]}}'; exit 1"#,
            rom.display()
        ),
    );
    let out = run(
        &d.path().join("r.db"),
        &cli,
        &["oysterav", "--ndjson", games.to_str().unwrap()],
    );
    assert_eq!(out.status.code(), Some(1));
    let ev = events(&out);
    assert_eq!(ev[1]["state"], "threat");
    assert_eq!(ev[1]["findings"][0]["threat"], "Bad.One");
    assert_eq!(ev[2]["threats"], 1);
    assert!(rom.exists(), "oysterAV only looks");
}

#[test]
fn an_unreadable_answer_is_an_error_not_a_threat() {
    let d = tempfile::tempdir().unwrap();
    let cli = fake(d.path(), "echo 'clamd is down' >&2; exit 2");
    let out = run(
        &d.path().join("r.db"),
        &cli,
        &["oysterav", "--ndjson", d.path().to_str().unwrap()],
    );
    assert_eq!(out.status.code(), Some(0));
    let ev = events(&out);
    assert_eq!(ev[1]["state"], "error");
    assert_eq!(ev[2]["clean"], false);
    assert_eq!(ev[2]["errors"], 1);
}

#[test]
fn the_library_folder_is_a_target_and_the_setting_is_kept() {
    let d = tempfile::tempdir().unwrap();
    let lib = d.path().join("library");
    std::fs::create_dir(&lib).unwrap();
    let db = d.path().join("r.db");
    let cli = fake(
        d.path(),
        r#"echo '{"event":"result","clean":true,"findings":[],"pack_errors":[]}'"#,
    );
    run(
        &db,
        &cli,
        &[
            "settings",
            "--library",
            lib.to_str().unwrap(),
            "--scan-downloads",
            "true",
        ],
    );
    let shown = String::from_utf8(run(&db, &cli, &["settings"]).stdout).unwrap();
    assert!(shown.contains("check downloads yes"), "{shown}");
    let out = run(&db, &cli, &["oysterav", "--ndjson", "games"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(events(&out)[1]["target"], "games");
    // Nothing to look at is said, not guessed.
    let none = run(&d.path().join("other.db"), &cli, &["oysterav"]);
    assert!(!none.status.success());
}
