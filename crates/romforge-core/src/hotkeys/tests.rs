use super::*;

fn start(a: Action) -> HotkeyEvent {
    HotkeyEvent::Start { action: a }
}

#[test]
fn a_chord_fires_once_when_its_last_button_goes_down() {
    let t = Instant::now();
    let mut c = Chords::new();
    assert!(c.press(Back, t).is_empty(), "Select alone does nothing");
    assert_eq!(c.press(Start, t), [start(Action::Quit)]);
    assert!(c.release(Start).is_empty(), "a one-shot has nothing to stop");
    assert_eq!(
        c.press(Start, t),
        [start(Action::Quit)],
        "again while Select is held"
    );
    c.release(Start);
    c.release(Back);
    assert!(c.press(Start, t).is_empty(), "Start alone does nothing");
    // Select pressed second completes the chord too.
    assert_eq!(c.press(Back, t), [start(Action::Quit)]);
}

#[test]
fn held_hotkeys_last_until_their_chord_is_let_go() {
    let t = Instant::now();
    let mut c = Chords::new();
    c.press(Back, t);
    assert_eq!(c.press(RightTrigger, t), [start(Action::FastForward)]);
    assert!(c.press(RightTrigger, t).is_empty(), "not twice while running");
    assert_eq!(
        c.release(Back),
        [HotkeyEvent::Stop {
            action: Action::FastForward
        }],
        "letting go of either button stops it"
    );
    assert!(c.release(RightTrigger).is_empty());
    c.press(Back, t);
    c.press(LeftTrigger, t);
    assert_eq!(
        c.release(LeftTrigger),
        [HotkeyEvent::Stop {
            action: Action::Rewind
        }]
    );
}

#[test]
fn both_sticks_open_the_emulator_menu_without_the_radial() {
    let t = Instant::now();
    let mut c = Chords::new();
    c.press(LeftStick, t);
    assert_eq!(c.press(RightStick, t), [start(Action::Menu)]);
    assert!(c.tick(t + RADIAL_HOLD * 2).is_empty(), "L3 isn't alone");
}

#[test]
fn holding_l3_opens_the_radial_and_the_right_stick_picks() {
    let t = Instant::now();
    let mut c = Chords::new();
    c.press(LeftStick, t);
    assert!(c.tick(t + Duration::from_millis(500)).is_empty());
    assert_eq!(c.tick(t + RADIAL_HOLD), [HotkeyEvent::RadialOpen]);
    assert!(c.radial_open());
    assert!(c.tick(t + RADIAL_HOLD * 2).is_empty(), "opens once");
    // Up is the first slot (Quit); right a third of the way round.
    assert_eq!(c.stick(0.0, 1.0), [HotkeyEvent::RadialSlot { slot: Some(0) }]);
    assert!(c.stick(0.05, 0.95).is_empty(), "still the same slot");
    assert_eq!(c.stick(0.87, -0.5), [HotkeyEvent::RadialSlot { slot: Some(2) }]);
    // Letting go of L3 does what it points at.
    assert_eq!(c.release(LeftStick), [HotkeyEvent::RadialClose, start(RADIAL[2])]);
    assert!(!c.radial_open());
}

#[test]
fn the_radial_cancels_in_the_middle_or_with_b() {
    let t = Instant::now();
    let mut c = Chords::new();
    c.press(LeftStick, t);
    c.tick(t + RADIAL_HOLD);
    c.stick(0.0, 1.0);
    assert_eq!(c.stick(0.1, 0.1), [HotkeyEvent::RadialSlot { slot: None }]);
    assert_eq!(c.release(LeftStick), [HotkeyEvent::RadialClose], "nothing chosen");
    c.press(LeftStick, t);
    c.tick(t + RADIAL_HOLD);
    c.stick(0.0, 1.0);
    assert_eq!(c.press(East, t), [HotkeyEvent::RadialClose], "B cancels");
    // A chooses as letting go of L3 does.
    c.release(East);
    c.release(LeftStick);
    c.press(LeftStick, t);
    c.tick(t + RADIAL_HOLD);
    c.stick(0.0, -1.0);
    assert_eq!(c.press(South, t), [HotkeyEvent::RadialClose, start(RADIAL[3])]);
}

#[test]
fn every_hotkey_is_in_the_table_once_and_gilrs_names_map() {
    for (i, h) in HOTKEYS.iter().enumerate() {
        assert!(h.chord.len() >= 2, "{h:?}");
        assert!(!HOTKEYS[i + 1..].iter().any(|o| o.action == h.action), "{h:?}");
    }
    for a in RADIAL {
        assert!(HOTKEYS.iter().any(|h| h.action == *a));
    }
    assert_eq!(PadButton::from_gilrs("Select"), Some(Back));
    assert_eq!(PadButton::from_gilrs("LeftTrigger2"), Some(LeftTrigger));
    assert_eq!(PadButton::from_gilrs("LeftThumb"), Some(LeftStick));
    assert_eq!(PadButton::from_gilrs("Mode"), None);
}
