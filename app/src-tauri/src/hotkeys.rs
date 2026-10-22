//! The pad while a game runs (stage 2, step 3): romforge reads the hotkey
//! chords itself ([`romforge_core::hotkeys`]) and makes the game do them,
//! each by its route ([`controls::route`]): a command to RetroArch, the
//! emulator's own binding (nothing to do here), or a quit (SIGTERM, as the
//! bar's Stop does). The radial menu shows in the overlay (`overlay.rs`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use romforge_core::controller::PadEvent;
use romforge_core::hotkeys::{Action, Chords, HotkeyEvent};
use romforge_core::launch::Session;
use romforge_core::launch::controls::{self, Remote};
use tauri::{AppHandle, Emitter, Manager};

use crate::play::Now;

/// The chords' state (one pad listener, one of these; each pad's buttons
/// kept apart in it), whether the hotkeys
/// are on for the game running (read when it started), and the command
/// channel of the game running (by its pid).
#[derive(Default)]
pub struct Watcher(Mutex<Chords>, AtomicBool, Mutex<Option<(u32, Arc<Remote>)>>);

/// The hotkeys on or off for the game starting now (Emulator Settings).
pub fn set_on(app: &AppHandle, on: bool) {
    app.state::<Watcher>().1.store(on, Ordering::Relaxed);
}

/// What the pad did, for the hotkeys. Only while a game runs; otherwise
/// the chords start again from nothing (a held button from before a game
/// doesn't count).
pub fn pad(app: &AppHandle, e: &PadEvent) {
    let watcher = app.state::<Watcher>();
    let mut chords = watcher.0.lock().unwrap_or_else(|e| e.into_inner());
    let session = now(app).filter(|s| s.is_running());
    let Some(session) = session.filter(|_| watcher.1.load(Ordering::Relaxed)) else {
        if chords.radial_open() {
            radial(app, &HotkeyEvent::RadialClose);
        }
        *chords = Chords::new();
        // A game that ended: its held commands stop with its channel.
        *watcher.2.lock().unwrap_or_else(|e| e.into_inner()) = None;
        return;
    };
    let now = Instant::now();
    let events = match *e {
        PadEvent::Button {
            pad,
            button,
            down: true,
        } => chords.press(pad, button, now),
        PadEvent::Button {
            pad,
            button,
            down: false,
        } => chords.release(pad, button),
        PadEvent::Stick { pad, x, y } => chords.stick(pad, x, y),
        PadEvent::Disconnected { pad, .. } => chords.disconnect(pad),
        PadEvent::Tick => chords.tick(now),
        _ => Vec::new(),
    };
    drop(chords);
    for ev in events {
        match ev {
            HotkeyEvent::Start { action } => start(&watcher, &session, action),
            HotkeyEvent::Stop { action } => {
                if let Some(r) = remote(&watcher, &session) {
                    r.stop(action);
                }
            }
            ev => radial(app, &ev),
        }
    }
}

/// The ring's window shown or hidden, and the ring told.
fn radial(app: &AppHandle, ev: &HotkeyEvent) {
    match ev {
        HotkeyEvent::RadialOpen => crate::overlay::show(app, true),
        HotkeyEvent::RadialClose => crate::overlay::show(app, false),
        _ => {}
    }
    let _ = app.emit("radial", ev);
}

fn now(app: &AppHandle) -> Option<Arc<Session>> {
    let now = app.state::<Now>();
    let guard = now.0.lock().unwrap_or_else(|e| e.into_inner());
    guard.clone()
}

/// The game's command channel, made the first time it is needed.
fn remote(watcher: &Watcher, s: &Session) -> Option<Arc<Remote>> {
    let channel = s.control?;
    let mut slot = watcher.2.lock().unwrap_or_else(|e| e.into_inner());
    match &*slot {
        Some((pid, r)) if *pid == s.playing.pid => Some(r.clone()),
        _ => {
            let r = Arc::new(Remote::new(channel));
            *slot = Some((s.playing.pid, r.clone()));
            Some(r)
        }
    }
}

/// Do a hotkey's action in the running game (`controls::act` decides how),
/// off the pad's thread.
fn start(watcher: &Watcher, s: &Arc<Session>, action: Action) {
    let r = remote(watcher, s);
    let s = s.clone();
    std::thread::spawn(move || controls::act(&s, r.as_deref(), action, crate::play::GRACE));
}

/// The Hotkeys tab: the table, the radial ring, and what each of these
/// emulators gets.
#[tauri::command]
pub async fn hotkeys_view(emulators: Vec<String>) -> Result<romforge_core::hotkeys::HotkeysView, String> {
    crate::db::blocking(move || Ok(romforge_core::hotkeys::view(emulators.iter().map(String::as_str)))).await
}
