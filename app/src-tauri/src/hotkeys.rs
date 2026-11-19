//! The pad while a game runs (stage 2, step 3): romforge reads the hotkey
//! chords itself ([`romforge_core::hotkeys`]) and makes the game do them
//! ([`romforge_core::hotkeys::Watcher`]: a command to RetroArch, the
//! emulator's own binding, or a quit). The radial menu shows in the
//! overlay (`overlay.rs`). For a game uQ's launcher provider started, the
//! provider does the same and this app runs as its overlay (`companion.rs`).

use std::sync::atomic::{AtomicBool, Ordering};

use romforge_core::controller::PadEvent;
use romforge_core::hotkeys::HotkeyEvent;
use tauri::{AppHandle, Emitter, Manager};

use crate::play::{GRACE, Now};

/// The chords' state, and whether the hotkeys are on for the game running
/// (read when it started).
#[derive(Default)]
pub struct Watcher(romforge_core::hotkeys::Watcher, AtomicBool);

/// The hotkeys on or off for the game starting now (Emulator Settings).
pub fn set_on(app: &AppHandle, on: bool) {
    app.state::<Watcher>().1.store(on, Ordering::Relaxed);
}

/// What the pad did, for the hotkeys. Only while a game runs; otherwise
/// the chords start again from nothing.
pub fn pad(app: &AppHandle, e: &PadEvent) {
    let watcher = app.state::<Watcher>();
    let game = now(app)
        .filter(|s| s.is_running())
        .filter(|_| watcher.1.load(Ordering::Relaxed));
    for ev in watcher.0.pad(game.as_ref(), e, GRACE) {
        radial(app, &ev);
    }
}

/// The ring's window shown or hidden, and the ring told.
pub fn radial(app: &AppHandle, ev: &HotkeyEvent) {
    match ev {
        HotkeyEvent::RadialOpen => crate::overlay::show(app, true),
        HotkeyEvent::RadialClose => crate::overlay::show(app, false),
        _ => {}
    }
    let _ = app.emit("radial", ev);
}

fn now(app: &AppHandle) -> Option<std::sync::Arc<romforge_core::launch::Session>> {
    let now = app.state::<Now>();
    let guard = now.0.lock().unwrap_or_else(|e| e.into_inner());
    guard.clone()
}

/// The Hotkeys tab: the table, the radial ring, and what each of these
/// emulators gets.
#[tauri::command]
pub async fn hotkeys_view(emulators: Vec<String>) -> Result<romforge_core::hotkeys::HotkeysView, String> {
    crate::db::blocking(move || Ok(romforge_core::hotkeys::view(emulators.iter().map(String::as_str)))).await
}
