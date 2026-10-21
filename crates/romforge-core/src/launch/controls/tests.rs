use std::time::Instant;

use super::super::owned::Format;
use super::*;

#[test]
fn each_emulator_gets_its_routes() {
    assert_eq!(route("retroarch:snes9x", Action::Rewind), Route::Command);
    assert_eq!(route("duckstation", Action::Quit), Route::Stop);
    assert_eq!(route("duckstation", Action::Rewind), Route::Bound);
    assert_eq!(route("pcsx2", Action::FastForward), Route::Bound);
    assert_eq!(route("pcsx2", Action::Rewind), Route::None);
    assert_eq!(route("scummvm", Action::Quit), Route::Stop);
    assert_eq!(route("scummvm", Action::SaveState), Route::None);
    assert_eq!(crate::hotkeys::support("scummvm"), [Action::Quit]);
    assert_eq!(crate::hotkeys::support("retroarch:mgba").len(), HOTKEYS.len());
    assert!(!crate::hotkeys::support("pcsx2").contains(&Action::Rewind));
}

#[test]
fn retroarch_gets_a_port_and_its_own_pad_hotkeys_unbound() {
    let (keys, remove, channel) = retroarch(true);
    assert!(remove.is_empty());
    let get = |k: &str| keys.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    let Some(Channel::RetroArch { port }) = channel else {
        panic!("no port")
    };
    assert_ne!(port, 0);
    assert_eq!(get("network_cmd_port"), Some(port.to_string().as_str()));
    assert_eq!(get("network_cmd_enable"), Some("true"));
    assert_eq!(get("input_enable_hotkey_btn"), Some("nul"));
    assert_eq!(get("input_quit_gamepad_combo"), Some("0"));

    // Off: all of it taken out, nothing set.
    let (keys, remove, channel) = retroarch(false);
    assert!(keys.is_empty() && channel.is_none());
    assert!(remove.iter().any(|k| k == "network_cmd_enable"));
    assert!(remove.iter().any(|k| k == "input_rewind_btn"));
}

#[test]
fn duckstation_and_pcsx2_bind_romforges_chords() {
    let write = || ConfigWrite {
        path: "settings.ini".into(),
        format: Format::Ini,
        keys: Vec::new(),
        remove: Vec::new(),
    };
    let mut w = [write()];
    bind("duckstation", true, &mut w);
    let get = |w: &ConfigWrite, k: &str| w.keys.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    assert_eq!(
        get(&w[0], "[Hotkeys]SaveSelectedSaveState").as_deref(),
        Some("SDL-0/Back & SDL-0/RightShoulder")
    );
    assert_eq!(
        get(&w[0], "[Hotkeys]Rewind").as_deref(),
        Some("SDL-0/Back & SDL-0/+LeftTrigger")
    );
    assert_eq!(
        get(&w[0], "[Hotkeys]OpenPauseMenu").as_deref(),
        Some("SDL-0/LeftStick & SDL-0/RightStick")
    );

    let mut w = [write()];
    bind("pcsx2", true, &mut w);
    assert_eq!(
        get(&w[0], "[Hotkeys]HoldTurbo").as_deref(),
        Some("SDL-0/Back & SDL-0/+RightTrigger")
    );
    assert!(get(&w[0], "[Hotkeys]Rewind").is_none());

    // Off: taken out again.
    let mut w = [write()];
    bind("pcsx2", false, &mut w);
    assert!(w[0].keys.is_empty());
    assert!(w[0].remove.contains(&"[Hotkeys]OpenPauseMenu".to_owned()));

    let mut w = [write()];
    bind("mgba", true, &mut w);
    assert!(w[0].keys.is_empty() && w[0].remove.is_empty());
}

/// A socket standing in for RetroArch, and what it heard within `wait`.
fn heard(sock: &UdpSocket, wait: Duration) -> Vec<String> {
    sock.set_read_timeout(Some(Duration::from_millis(20))).unwrap();
    let until = Instant::now() + wait;
    let mut out = Vec::new();
    let mut buf = [0u8; 64];
    while Instant::now() < until {
        if let Ok(n) = sock.recv(&mut buf) {
            out.push(String::from_utf8_lossy(&buf[..n]).into_owned());
        }
    }
    out
}

#[test]
fn retroarch_hears_commands_and_held_ones_until_let_go() {
    let sock = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    let port = sock.local_addr().unwrap().port();
    let r = Remote::new(Channel::RetroArch { port });

    r.start(Action::SaveState).unwrap();
    r.start(Action::Quit).unwrap();
    assert_eq!(
        heard(&sock, Duration::from_millis(100)),
        ["SAVE_STATE", "QUIT", "QUIT"]
    );

    r.start(Action::FastForward).unwrap();
    r.start(Action::FastForward).unwrap();
    let held = heard(&sock, Duration::from_millis(200));
    assert!(held.len() >= 5, "{held:?}");
    assert!(held.iter().all(|c| c == "FAST_FORWARD_HOLD"));
    r.stop(Action::FastForward);
    std::thread::sleep(Duration::from_millis(50));
    heard(&sock, Duration::from_millis(30));
    assert!(heard(&sock, Duration::from_millis(100)).is_empty(), "stopped");

    // A game ending while rewind is held stops it too.
    r.start(Action::Rewind).unwrap();
    assert!(!heard(&sock, Duration::from_millis(60)).is_empty());
    drop(r);
    std::thread::sleep(Duration::from_millis(50));
    heard(&sock, Duration::from_millis(30));
    assert!(heard(&sock, Duration::from_millis(100)).is_empty(), "dropped");
}
