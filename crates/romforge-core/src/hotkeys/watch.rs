//! Doing what the pad's hotkeys say to the game running: the part of the
//! hotkeys that is the same wherever the game was started from. The app
//! and uQ's launcher provider each feed it the pad ([`Watcher::pad`]) and
//! show the radial menu's events their own way (a window of the app, or
//! the app's overlay as a companion process).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{Action, Chords, HotkeyEvent};
use crate::controller::PadEvent;
use crate::launch::Session;
use crate::launch::controls::{self, Remote};

/// The chords' state (one pad listener, one of these; each pad's buttons
/// kept apart in it) and the command channel of the game running (by its
/// pid).
#[derive(Default)]
pub struct Watcher {
    chords: Mutex<Chords>,
    remote: Mutex<Option<(u32, Arc<Remote>)>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Watcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// What the pad did. `game` is the game the hotkeys are on for, if
    /// there is one: its hotkeys' actions are done at once (off this
    /// thread when they wait, as a quit does). Without one the chords start
    /// again from nothing (a button held from before a game doesn't count),
    /// and a radial menu left open is closed. What is returned is the
    /// radial menu's events, for whoever shows it; a quit asked for
    /// stops the game after `grace` if it hasn't quit by then.
    pub fn pad(&self, game: Option<&Arc<Session>>, e: &PadEvent, grace: Duration) -> Vec<HotkeyEvent> {
        let mut chords = lock(&self.chords);
        let Some(session) = game else {
            let closed = chords.radial_open().then_some(HotkeyEvent::RadialClose);
            *chords = Chords::new();
            // A game that ended: its held commands stop with its channel.
            *lock(&self.remote) = None;
            return closed.into_iter().collect();
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
        let mut radial = Vec::new();
        for ev in events {
            match ev {
                HotkeyEvent::Start { action } => self.start(session, action, grace),
                HotkeyEvent::Stop { action } => {
                    if let Some(r) = self.remote(session) {
                        r.stop(action);
                    }
                }
                ev => radial.push(ev),
            }
        }
        radial
    }

    /// The game's command channel, made the first time it is needed.
    fn remote(&self, s: &Session) -> Option<Arc<Remote>> {
        let channel = s.control?;
        let mut slot = lock(&self.remote);
        match &*slot {
            Some((pid, r)) if *pid == s.playing.pid => Some(r.clone()),
            _ => {
                let r = Arc::new(Remote::new(channel));
                *slot = Some((s.playing.pid, r.clone()));
                Some(r)
            }
        }
    }

    /// Do a hotkey's action in the game (`controls::act` decides how), on
    /// a thread of its own.
    fn start(&self, s: &Arc<Session>, action: Action, grace: Duration) {
        let r = self.remote(s);
        let s = s.clone();
        std::thread::spawn(move || controls::act(&s, r.as_deref(), action, grace));
    }
}

#[cfg(test)]
mod tests;
