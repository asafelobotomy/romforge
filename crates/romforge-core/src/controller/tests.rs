use super::*;

#[test]
fn steam_s_virtual_pad_wins_and_ignored_pads_go() {
    let xbox = PadId {
        id: 0,
        vendor: Some(0x045e),
        product: Some(0x028e),
    };
    let ds = PadId {
        id: 1,
        vendor: Some(0x054c),
        product: Some(0x0ce6),
    };
    let steam = PadId {
        id: 2,
        vendor: Some(0x28de),
        product: Some(0x11ff),
    };
    assert_eq!(usable(&[xbox, ds], &[]), [0, 1]);
    assert_eq!(
        usable(&[xbox, ds, steam], &[]),
        [2],
        "each physical pad would come twice"
    );
    let ignore = parse_ignore("0x045e/0x028e, 0X054C/0X0CE6,bogus");
    assert_eq!(ignore, [(0x045e, 0x028e), (0x054c, 0x0ce6)]);
    assert_eq!(
        usable(
            &[
                xbox,
                ds,
                PadId {
                    id: 3,
                    vendor: None,
                    product: None
                }
            ],
            &ignore
        ),
        [3]
    );
}

#[test]
fn buttons_map_to_actions() {
    assert_eq!(action_of("South"), Some(Action::Accept));
    assert_eq!(action_of("East"), Some(Action::Back));
    assert_eq!(action_of("Start"), Some(Action::Menu));
    assert_eq!(action_of("LeftTrigger2"), Some(Action::First));
    assert_eq!(
        action_of("Mode"),
        None,
        "the guide button stays Steam's / the system's"
    );
}

#[test]
fn a_held_direction_repeats_and_a_stick_has_slack() {
    let t0 = Instant::now();
    let ms = |n| t0 + Duration::from_millis(n);
    let mut d = Directions::default();
    assert_eq!(d.press(Action::Down, t0), Action::Down);
    assert_eq!(d.tick(ms(300)), None, "not yet");
    assert_eq!(d.tick(ms(400)), Some(Action::Down));
    assert_eq!(d.tick(ms(450)), None);
    assert_eq!(d.tick(ms(510)), Some(Action::Down));
    d.release(Action::Down);
    assert_eq!(d.tick(ms(2000)), None);

    // The stick: past 0.6 is a press; it holds until back under 0.35.
    let mut d = Directions::default();
    assert_eq!(d.stick(true, 0.5, t0), None);
    assert_eq!(d.stick(true, 0.7, t0), Some(Action::Right));
    assert_eq!(d.stick(true, 0.45, ms(10)), None, "still held");
    assert_eq!(d.tick(ms(400)), Some(Action::Right));
    assert_eq!(d.stick(true, 0.1, ms(420)), None, "let go");
    assert_eq!(d.tick(ms(900)), None);
    assert_eq!(d.stick(false, 0.9, ms(950)), Some(Action::Up), "gilrs's Y is up");
    assert_eq!(
        d.stick(false, -0.9, ms(960)),
        None,
        "flipped at once: still Up until it passes the middle"
    );
}
