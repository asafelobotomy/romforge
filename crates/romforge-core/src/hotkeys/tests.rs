use super::*;

fn start(a: Action) -> HotkeyEvent {
    HotkeyEvent::Start { action: a }
}

#[test]
fn a_chord_fires_once_when_its_last_button_goes_down() {
    let t = Instant::now();
    let mut c = Chords::new();
    assert!(c.press(0, Back, t).is_empty(), "Select alone does nothing");
    assert_eq!(c.press(0, Start, t), [start(Action::Quit)]);
    assert!(c.release(0, Start).is_empty(), "a one-shot has nothing to stop");
    assert_eq!(
        c.press(0, Start, t),
        [start(Action::Quit)],
        "again while Select is held"
    );
    c.release(0, Start);
    c.release(0, Back);
    assert!(c.press(0, Start, t).is_empty(), "Start alone does nothing");
    // Select pressed second completes the chord too.
    assert_eq!(c.press(0, Back, t), [start(Action::Quit)]);
}

#[test]
fn held_hotkeys_last_until_their_chord_is_let_go() {
    let t = Instant::now();
    let mut c = Chords::new();
    c.press(0, Back, t);
    assert_eq!(c.press(0, RightTrigger, t), [start(Action::FastForward)]);
    assert!(c.press(0, RightTrigger, t).is_empty(), "not twice while running");
    assert_eq!(
        c.release(0, Back),
        [HotkeyEvent::Stop {
            action: Action::FastForward
        }],
        "letting go of either button stops it"
    );
    assert!(c.release(0, RightTrigger).is_empty());
    c.press(0, Back, t);
    c.press(0, LeftTrigger, t);
    assert_eq!(
        c.release(0, LeftTrigger),
        [HotkeyEvent::Stop {
            action: Action::Rewind
        }]
    );
}

#[test]
fn both_sticks_open_the_emulator_menu_without_the_radial() {
    let t = Instant::now();
    let mut c = Chords::new();
    c.press(0, LeftStick, t);
    assert_eq!(c.press(0, RightStick, t), [start(Action::Menu)]);
    assert!(c.tick(t + RADIAL_HOLD * 2).is_empty(), "L3 isn't alone");
}

#[test]
fn holding_l3_opens_the_radial_and_the_right_stick_picks() {
    let t = Instant::now();
    let mut c = Chords::new();
    c.press(0, LeftStick, t);
    assert!(c.tick(t + Duration::from_millis(500)).is_empty());
    assert_eq!(c.tick(t + RADIAL_HOLD), [HotkeyEvent::RadialOpen]);
    assert!(c.radial_open());
    assert!(c.tick(t + RADIAL_HOLD * 2).is_empty(), "opens once");
    // Up is the first slot (Quit); right a quarter of the way round.
    assert_eq!(c.stick(0, 0.0, 1.0), [HotkeyEvent::RadialSlot { slot: Some(0) }]);
    assert!(c.stick(0, 0.05, 0.95).is_empty(), "still the same slot");
    assert_eq!(c.stick(0, 1.0, 0.0), [HotkeyEvent::RadialSlot { slot: Some(1) }]);
    // Letting go of L3 does what it points at.
    assert_eq!(
        c.release(0, LeftStick),
        [HotkeyEvent::RadialClose, start(RADIAL[1])]
    );
    assert!(!c.radial_open());
}

#[test]
fn the_radial_cancels_in_the_middle_or_with_b() {
    let t = Instant::now();
    let mut c = Chords::new();
    c.press(0, LeftStick, t);
    c.tick(t + RADIAL_HOLD);
    c.stick(0, 0.0, 1.0);
    assert_eq!(c.stick(0, 0.1, 0.1), [HotkeyEvent::RadialSlot { slot: None }]);
    assert_eq!(
        c.release(0, LeftStick),
        [HotkeyEvent::RadialClose],
        "nothing chosen"
    );
    c.press(0, LeftStick, t);
    c.tick(t + RADIAL_HOLD);
    c.stick(0, 0.0, 1.0);
    assert_eq!(c.press(0, East, t), [HotkeyEvent::RadialClose], "B cancels");
    // A chooses as letting go of L3 does.
    c.release(0, East);
    c.release(0, LeftStick);
    c.press(0, LeftStick, t);
    c.tick(t + RADIAL_HOLD);
    c.stick(0, 0.0, -1.0);
    assert_eq!(c.press(0, South, t), [HotkeyEvent::RadialClose, start(RADIAL[2])]);
}

#[test]
fn every_hotkey_is_in_the_table_once_and_gilrs_names_map() {
    for (i, h) in HOTKEYS.iter().enumerate() {
        assert!(h.chord.len() >= 2, "{h:?}");
        assert!(!HOTKEYS[i + 1..].iter().any(|o| o.action == h.action), "{h:?}");
    }
    for a in RADIAL {
        let h = HOTKEYS.iter().find(|h| h.action == *a).expect("in the table");
        assert!(!h.hold, "{a:?}: the ring can't stop a held one");
    }
    assert_eq!(PadButton::from_gilrs("Select"), Some(Back));
    assert_eq!(PadButton::from_gilrs("LeftTrigger2"), Some(LeftTrigger));
    assert_eq!(PadButton::from_gilrs("LeftThumb"), Some(LeftStick));
    assert_eq!(PadButton::from_gilrs("Mode"), None);
}

#[test]
fn a_chord_is_on_one_pad_and_a_pad_going_lets_go_of_its_buttons() {
    let t = Instant::now();
    let mut c = Chords::new();
    c.press(0, Back, t);
    assert!(
        c.press(1, Start, t).is_empty(),
        "Select on one pad, Start on another"
    );
    // Pad 0 goes while Select and R2 are held: fast forward stops, and
    // Start alone (pad 0 back again) doesn't quit.
    assert_eq!(c.press(0, RightTrigger, t), [start(Action::FastForward)]);
    assert_eq!(
        c.disconnect(0),
        [HotkeyEvent::Stop {
            action: Action::FastForward
        }]
    );
    assert!(c.press(0, Start, t).is_empty());
    // A radial menu its pad took away closes without choosing.
    let mut c = Chords::new();
    c.press(1, LeftStick, t);
    c.tick(t + RADIAL_HOLD);
    assert!(c.stick(0, 0.0, 1.0).is_empty(), "only its own pad points");
    c.stick(1, 0.0, 1.0);
    assert_eq!(c.disconnect(1), [HotkeyEvent::RadialClose]);
    assert!(!c.radial_open());
}

#[test]
fn the_ring_offers_only_what_romforge_does_itself() {
    assert_eq!(ring("retroarch:mgba"), RADIAL);
    assert_eq!(
        ring("duckstation"),
        [Action::Quit],
        "its own bindings react to chords only"
    );
    assert_eq!(ring("scummvm"), [Action::Quit]);
    let v = view(["pcsx2"]);
    assert_eq!(v.support[0].ring, [Action::Quit]);
    assert_eq!(v.quit_chord, "Select+Start");
    assert_eq!(v.hint, "Select+Start quits · hold L3 for the menu");
    assert_eq!(v.radial_button, "L3");
}
