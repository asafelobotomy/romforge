//! Milestone 18, 1.6: a running game is asked to quit before it is killed,
//! and its play time is kept (stand-in emulators).
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use romforge_core::Library;
use romforge_core::launch::{Ended, LaunchSpec, Session};

fn spec(dir: &Path, body: &str) -> LaunchSpec {
    let program = dir.join("emu");
    std::fs::write(&program, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    LaunchSpec {
        game: "Test GBA".into(),
        system: "Game Boy Advance".into(),
        emulator: "stand-in".into(),
        rom: dir.join("game.gba"),
        program,
        args: Vec::new(),
        env: Vec::new(),
        prepare: Default::default(),
        skipped: Vec::new(),
    }
}

#[test]
fn a_game_is_asked_to_quit_first() {
    let t = tempfile::tempdir().unwrap();
    let saved = t.path().join("saved");
    // Quits cleanly on SIGTERM, after "saving".
    let s = Session::start(&spec(
        t.path(),
        &format!(
            "trap 'echo saved > {}; exit 0' TERM\nwhile true; do sleep 0.1; done",
            saved.display()
        ),
    ))
    .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(s.is_running());
    assert!(!s.stop(Duration::from_secs(5)).unwrap(), "it quit by itself");
    assert_eq!(std::fs::read_to_string(&saved).unwrap().trim(), "saved");
    let ended = s.wait();
    assert_eq!(ended.code, Some(0));

    // Ignores SIGTERM: killed once the grace time is over.
    let stubborn = Session::start(&spec(t.path(), "trap '' TERM\nwhile true; do sleep 0.1; done")).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(stubborn.stop(Duration::from_millis(400)).unwrap(), "killed");
    assert!(!stubborn.is_running());
}

#[test]
fn play_time_is_kept() {
    let mut lib = Library::open_in_memory().unwrap();
    let t = tempfile::tempdir().unwrap();
    let s = Session::start(&spec(t.path(), "exit 0")).unwrap();
    let quick = s.wait();
    assert!(
        !lib.record_play(&quick).unwrap(),
        "a run under ten seconds isn't counted"
    );

    let long = |seconds, started| Ended {
        playing: romforge_core::launch::Playing {
            started,
            ..quick.playing.clone()
        },
        seconds,
        code: Some(0),
    };
    assert!(lib.record_play(&long(600, 1_000)).unwrap());
    assert!(lib.record_play(&long(300, 2_000)).unwrap());
    let stats = lib.play_stats("Game Boy Advance", "Test GBA").unwrap().unwrap();
    assert_eq!(
        (stats.seconds, stats.sessions, stats.last_played),
        (900, 2, 2_000)
    );
}
