use super::*;
use crate::hotkeys::PadButton::{Back, LeftStick, Start};
use crate::launch::LaunchSpec;

fn sleeper(seconds: &str) -> Arc<Session> {
    let spec = LaunchSpec {
        game: "Game".into(),
        system: "NES".into(),
        emulator: "standin".into(),
        rom: "game.nes".into(),
        program: "/bin/sleep".into(),
        args: vec![seconds.into()],
        env: Vec::new(),
        prepare: Default::default(),
        skipped: Vec::new(),
    };
    Arc::new(Session::start(&spec).unwrap())
}

fn button(button: crate::hotkeys::PadButton, down: bool) -> PadEvent {
    PadEvent::Button { pad: 0, button, down }
}

fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    for _ in 0..100 {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}");
}

#[test]
fn select_and_start_quit_the_game() {
    let (w, game) = (Watcher::new(), sleeper("30"));
    let grace = Duration::from_secs(5);
    assert!(w.pad(Some(&game), &button(Back, true), grace).is_empty());
    assert!(game.is_running());
    assert!(w.pad(Some(&game), &button(Start, true), grace).is_empty());
    wait_for("the game to be stopped", || !game.is_running());
}

#[test]
fn the_radial_menu_is_for_whoever_shows_it() {
    let (w, game) = (Watcher::new(), sleeper("30"));
    let grace = Duration::from_secs(5);
    w.pad(Some(&game), &button(LeftStick, true), grace);
    std::thread::sleep(crate::hotkeys::RADIAL_HOLD + Duration::from_millis(100));
    assert_eq!(
        w.pad(Some(&game), &PadEvent::Tick, grace),
        [HotkeyEvent::RadialOpen]
    );
    // The game's gone (or hotkeys off): the ring closes, once.
    assert_eq!(w.pad(None, &PadEvent::Tick, grace), [HotkeyEvent::RadialClose]);
    assert!(w.pad(None, &PadEvent::Tick, grace).is_empty());
    let _ = game.stop(grace);
}

#[test]
fn nothing_held_before_a_game_counts() {
    let (w, game) = (Watcher::new(), sleeper("30"));
    let grace = Duration::from_secs(5);
    w.pad(None, &button(Back, true), grace);
    w.pad(Some(&game), &button(Start, true), grace);
    std::thread::sleep(Duration::from_millis(300));
    assert!(game.is_running(), "Select was held before the game, not during");
    let _ = game.stop(grace);
}
