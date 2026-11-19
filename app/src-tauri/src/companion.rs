//! `romforge-app --overlay[=EMULATOR]`: the app as the radial menu's
//! overlay for a game uQ's launcher provider started, with no window of
//! its own and none of the library. The provider reads the pad and does
//! the hotkeys (`romforge_core::hotkeys::Watcher`); this only draws. It is
//! sent the radial menu's events, one JSON line each on its standard
//! input, and quits when that closes (the game ended, or the provider did).

use std::io::BufRead;

use romforge_core::hotkeys::HotkeyEvent;
/// The emulator the overlay is for, when it was started as the companion
/// of a game that is not the app's own (what the ring can do depends on it).
#[derive(Default)]
pub struct Companion(pub Option<String>);

/// Whether `args` (without the program's name) ask for the overlay alone,
/// and for which emulator (`--overlay=retroarch:snes9x`).
pub fn from_args<S: AsRef<str>>(args: impl IntoIterator<Item = S>) -> Option<Companion> {
    args.into_iter().find_map(|a| {
        let a = a.as_ref();
        match a.strip_prefix("--overlay") {
            Some("") => Some(Companion(None)),
            Some(rest) => rest
                .strip_prefix('=')
                .map(|e| Companion(Some(e.to_owned()).filter(|e| !e.is_empty()))),
            None => None,
        }
    })
}

/// The emulator the ring is for: the companion's, else the game the app
/// itself is running.
#[tauri::command]
pub fn overlay_emulator(
    companion: tauri::State<'_, Companion>,
    now: tauri::State<'_, crate::play::Now>,
) -> Option<String> {
    companion.0.clone().or_else(|| {
        let now = now.0.lock().unwrap_or_else(|e| e.into_inner());
        now.as_ref()
            .filter(|s| s.is_running())
            .map(|s| s.playing.emulator.clone())
    })
}

/// One line of the provider's, as an event (anything else is skipped).
pub fn parse(line: &str) -> Option<HotkeyEvent> {
    serde_json::from_str(line.trim()).ok()
}

/// Read the events from standard input and show them; the app ends with it.
pub fn listen(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if let Some(ev) = parse(&line) {
                crate::hotkeys::radial(&app, &ev);
            }
        }
        // Closed: the game is over. The ring goes with it.
        crate::hotkeys::radial(&app, &HotkeyEvent::RadialClose);
        app.exit(0);
    });
}

/// Run as the overlay alone: the window the config makes is left out, and
/// only what the ring shows can be asked of the app. The pad is the
/// provider's: nothing here listens to it.
pub fn run(mut context: tauri::Context, overlay: Companion) {
    context.config_mut().app.windows.clear();
    tauri::Builder::default()
        .manage(overlay)
        .manage(crate::play::Now::default())
        .setup(|app| {
            crate::overlay::prepare(app.handle());
            listen(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::play::now_playing,
            crate::hotkeys::hotkeys_view,
            overlay_emulator
        ])
        .run(context)
        .expect("error while running romforge's overlay");
}

#[cfg(test)]
mod tests {
    use super::*;
    use romforge_core::hotkeys::Action;

    #[test]
    fn the_overlay_flag_and_its_emulator() {
        assert!(from_args(["--fullscreen"]).is_none());
        assert_eq!(from_args(["--overlay"]).unwrap().0, None);
        assert_eq!(from_args(["--overlay="]).unwrap().0, None);
        assert_eq!(
            from_args(["x", "--overlay=retroarch:snes9x"])
                .unwrap()
                .0
                .as_deref(),
            Some("retroarch:snes9x")
        );
        assert!(from_args(["--overlays"]).is_none());
    }

    #[test]
    fn lines_are_hotkey_events() {
        assert_eq!(parse(r#"{"kind":"radial_open"}"#), Some(HotkeyEvent::RadialOpen));
        assert_eq!(
            parse("{\"kind\":\"radial_slot\",\"slot\":2}\n"),
            Some(HotkeyEvent::RadialSlot { slot: Some(2) })
        );
        assert_eq!(
            parse(r#"{"kind":"start","action":"quit"}"#),
            Some(HotkeyEvent::Start { action: Action::Quit })
        );
        assert_eq!(parse("not json"), None);
        assert_eq!(parse(""), None);
    }
}
