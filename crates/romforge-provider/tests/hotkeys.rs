//! Hotkeys and the radial menu for a game the provider started: the pad
//! is fed to it by hand (no pad needed), the ring's overlay is a stand-in
//! for the app that keeps what it is sent, and the game is a stand-in
//! emulator.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use romforge_core::controller::PadEvent;
use romforge_core::hotkeys::PadButton::{Back, LeftStick, Start};
use romforge_core::launch::UserEmulator;
use romforge_core::{Library, Scope};
use romforge_provider::Games;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn library(dir: &Path, ran: &Path, seconds: u32) -> PathBuf {
    let db = dir.join("romforge.db");
    let mut lib = Library::open(&db).unwrap();
    lib.import_dat(fixtures().join("dats/nes.dat"), None).unwrap();
    lib.scan(fixtures().join("roms/nes"), Scope::All).unwrap();
    let program = dir.join("nesemu");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}.part'\nmv '{0}.part' '{0}'\nsleep {seconds}\n",
            ran.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    lib.add_emulator(&UserEmulator {
        name: "nesemu".into(),
        system: "NES".into(),
        program: program.display().to_string(),
        args: vec!["{rom}".into()],
        archives: false,
    })
    .unwrap();
    db
}

fn button(button: romforge_core::hotkeys::PadButton, down: bool) -> PadEvent {
    PadEvent::Button { pad: 0, button, down }
}

fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let by = Instant::now() + Duration::from_secs(10);
    while !f() {
        assert!(Instant::now() < by, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A stand-in for the app: it keeps its argument and whatever it is sent.
fn fake_app(dir: &Path, got: &Path) -> PathBuf {
    let app = dir.join("fakeapp");
    std::fs::write(
        &app,
        format!(
            "#!/bin/sh\necho \"$1\" > '{0}'\ncat >> '{0}'\necho closed >> '{0}'\n",
            got.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
    app
}

fn first_game(db: &Path) -> String {
    let lib = Library::open(db).unwrap();
    let hits = lib.launcher_search("test nes").unwrap();
    hits.into_iter().next().expect("a game in the fixtures").id
}

#[test]
fn the_pad_quits_the_game_and_the_app_draws_the_ring() {
    let dir = tempfile::tempdir().unwrap();
    let (ran, got) = (dir.path().join("ran"), dir.path().join("got"));
    let db = library(dir.path(), &ran, 60);
    let games = Arc::new(Games::new(db.clone()).with_app(Some(fake_app(dir.path(), &got))));
    let id = first_game(&db);
    let (tx, ended) = std::sync::mpsc::channel();
    games.play(&id, move |e| tx.send(e.clone()).unwrap()).unwrap();
    wait_for("the game", || ran.exists());
    // The ring's overlay was started for this game's emulator.
    wait_for("the overlay", || {
        std::fs::read_to_string(&got).is_ok_and(|t| t.starts_with("--overlay=nesemu"))
    });

    // Hold L3: the ring opens and the overlay is told.
    games.pad(&button(LeftStick, true));
    std::thread::sleep(Duration::from_millis(2100));
    games.pad(&PadEvent::Tick);
    wait_for("the ring to open", || {
        std::fs::read_to_string(&got).is_ok_and(|t| t.contains(r#"{"kind":"radial_open"}"#))
    });
    games.pad(&button(LeftStick, false));

    // Select + Start: quit.
    games.pad(&button(Back, true));
    games.pad(&button(Start, true));
    let end = ended
        .recv_timeout(Duration::from_secs(20))
        .expect("the game to end");
    assert!(!end.failed);
    // The game over, the app's pipe closes and it quits.
    wait_for("the overlay to be told to close", || {
        std::fs::read_to_string(&got).is_ok_and(|t| t.trim_end().ends_with("closed"))
    });
    assert!(!games.playing());
}

#[test]
fn without_the_app_the_game_still_plays_and_the_hotkeys_work() {
    let dir = tempfile::tempdir().unwrap();
    let ran = dir.path().join("ran");
    let db = library(dir.path(), &ran, 60);
    let games = Arc::new(Games::new(db.clone()).with_app(Some(dir.path().join("no-such-app"))));
    let id = first_game(&db);
    let (tx, ended) = std::sync::mpsc::channel();
    games.play(&id, move |e| tx.send(e.clone()).unwrap()).unwrap();
    wait_for("the game", || ran.exists());
    games.pad(&button(Back, true));
    games.pad(&button(Start, true));
    ended
        .recv_timeout(Duration::from_secs(20))
        .expect("the game to end");
}

#[test]
fn nothing_the_pad_does_counts_when_no_game_runs() {
    let dir = tempfile::tempdir().unwrap();
    let games = Games::new(dir.path().join("none.db"));
    games.pad(&button(Back, true));
    games.pad(&button(Start, true));
    games.pad(&PadEvent::Tick);
    assert!(!games.playing());
}
