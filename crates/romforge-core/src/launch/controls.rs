//! Stage 2, step 3: how romforge's hotkeys ([`crate::hotkeys`]) reach the
//! emulator running a game. Read in each emulator's source first:
//!
//! - RetroArch 1.22.2: plain-text UDP commands (`command.c`), on when
//!   `network_cmd_enable` is set. It listens on every interface (only
//!   master has `network_cmd_bind_address`), so romforge picks a free port
//!   for each game. Fast forward and rewind held last one frame per
//!   command, so romforge sends them again and again while held. `QUIT`
//!   asks to confirm (`confirm_quit`), so it is sent twice. RetroArch's
//!   own pad hotkeys are unbound (`"nul"`) so a chord never does two
//!   things.
//! - DuckStation and PCSX2 have no command channel for these (PCSX2's PINE
//!   saves and loads states only), so romforge writes their own
//!   `[Hotkeys]` bindings for exactly romforge's chords and they do the
//!   action themselves. Their default hotkeys are keyboard only, so there
//!   are no pad hotkeys to take out. A chord is `&`-joined parts; a
//!   trigger is the axis half `+LeftTrigger` (`SDLInputSource.cpp`).
//!   PCSX2 has no rewind; DuckStation's needs Rewind on in its settings.
//! - Quit, for every other emulator: romforge asks it to quit (SIGTERM, as
//!   the bar's Stop does).
//!
//! With the hotkeys off, every key here is taken out of romforge's copy of
//! the config, so the emulator's own defaults apply.

use std::net::UdpSocket;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::owned::ConfigWrite;
use crate::hotkeys::{Action, HOTKEYS, PadButton};

/// How romforge reaches a running emulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Channel {
    /// RetroArch's UDP command port, on this machine.
    RetroArch { port: u16 },
}

/// How an action reaches the emulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// romforge sends it over the emulator's [`Channel`].
    Command,
    /// The emulator's own binding for romforge's chord does it.
    Bound,
    /// romforge asks the emulator to quit (SIGTERM).
    Stop,
    /// Not for this emulator.
    None,
}

/// How `a` reaches `emulator` (its settings name, or `retroarch:<core>`).
pub fn route(emulator: &str, a: Action) -> Route {
    match (family(emulator), a) {
        ("retroarch", _) => Route::Command,
        ("duckstation", Action::Quit) | ("pcsx2", Action::Quit) => Route::Stop,
        ("duckstation", _) => Route::Bound,
        ("pcsx2", Action::Rewind) => Route::None,
        ("pcsx2", _) => Route::Bound,
        (_, Action::Quit) => Route::Stop,
        _ => Route::None,
    }
}

/// What the Hotkeys tab says of `emulator`'s hotkeys, beyond the list.
pub fn note(emulator: &str) -> Option<&'static str> {
    Some(match family(emulator) {
        "retroarch" => {
            "Rewind works once Rewind is on in Emulator Settings. The game also sees the chord's buttons."
        }
        "duckstation" => "Rewind works once Rewind is on in DuckStation's settings.",
        "pcsx2" => "PCSX2 has no rewind.",
        _ => return None,
    })
}

/// How long a quit sent over the command channel gets before romforge
/// asks the usual way (SIGTERM) too.
const QUIT_HEARD: Duration = Duration::from_secs(3);

/// Do a hotkey's action (a [`crate::hotkeys::HotkeyEvent::Start`]) in the
/// game `s` runs, by its [`route`]: a command over `remote` (its channel,
/// if it has one), a quit, or nothing (the emulator's own binding does
/// it). A quit the emulator didn't hear (or is still asking about) is then
/// asked the usual way, and stopped hard after `grace`: this blocks for a
/// quit, so call it on a thread of its own.
pub fn act(s: &super::Session, remote: Option<&Remote>, action: Action, grace: Duration) {
    match route(&s.playing.emulator, action) {
        Route::Command => match remote {
            Some(r) => {
                let sent = r.start(action).is_ok();
                if action == Action::Quit {
                    if sent {
                        std::thread::sleep(QUIT_HEARD);
                    }
                    let _ = s.stop(grace);
                }
            }
            // No channel (no RomForge folder): quit still works.
            None if action == Action::Quit => {
                let _ = s.stop(grace);
            }
            None => {}
        },
        Route::Stop => {
            let _ = s.stop(grace);
        }
        Route::Bound | Route::None => {}
    }
}

fn family(emulator: &str) -> &str {
    emulator.split(':').next().unwrap_or(emulator)
}

/// RetroArch's own pad hotkeys, unbound while romforge's are on.
const RETROARCH_UNBIND: &[&str] = &[
    "input_enable_hotkey_btn",
    "input_exit_emulator_btn",
    "input_menu_toggle_btn",
    "input_save_state_btn",
    "input_load_state_btn",
    "input_toggle_fast_forward_btn",
    "input_hold_fast_forward_btn",
    "input_rewind_btn",
];
const RETROARCH_COMBOS: &[&str] = &["input_menu_toggle_gamepad_combo", "input_quit_gamepad_combo"];

/// RetroArch's keys for the hotkeys, the ones to take out, and the port
/// (none when the hotkeys are off, or no port was free).
pub(super) fn retroarch(on: bool) -> (Vec<(String, String)>, Vec<String>, Option<Channel>) {
    let port = on.then(free_port).flatten();
    let all = || {
        RETROARCH_UNBIND
            .iter()
            .chain(RETROARCH_COMBOS)
            .chain(&["network_cmd_enable", "network_cmd_port"])
            .map(|k| (*k).to_owned())
    };
    let Some(port) = port else {
        return (Vec::new(), all().collect(), None);
    };
    let mut keys: Vec<(String, String)> = RETROARCH_UNBIND
        .iter()
        .map(|k| ((*k).to_owned(), "nul".to_owned()))
        .collect();
    keys.extend(RETROARCH_COMBOS.iter().map(|k| ((*k).to_owned(), "0".to_owned())));
    keys.push(("network_cmd_enable".into(), "true".into()));
    keys.push(("network_cmd_port".into(), port.to_string()));
    (keys, Vec::new(), Some(Channel::RetroArch { port }))
}

/// A UDP port free on this machine right now.
fn free_port() -> Option<u16> {
    UdpSocket::bind(("127.0.0.1", 0))
        .and_then(|s| s.local_addr())
        .ok()
        .map(|a| a.port())
}

/// The emulator's own hotkey names for romforge's actions.
fn bound_names(emulator: &str) -> &'static [(Action, &'static str)] {
    match emulator {
        "duckstation" => &[
            (Action::Menu, "OpenPauseMenu"),
            (Action::SaveState, "SaveSelectedSaveState"),
            (Action::LoadState, "LoadSelectedSaveState"),
            (Action::FastForward, "FastForward"),
            (Action::Rewind, "Rewind"),
        ],
        "pcsx2" => &[
            (Action::Menu, "OpenPauseMenu"),
            (Action::SaveState, "SaveStateToSlot"),
            (Action::LoadState, "LoadStateFromSlot"),
            (Action::FastForward, "HoldTurbo"),
        ],
        _ => &[],
    }
}

/// romforge's chord for `a`, as DuckStation and PCSX2 bind it.
fn sdl_chord(a: Action) -> Option<String> {
    let h = HOTKEYS.iter().find(|h| h.action == a)?;
    let part = |b: &PadButton| match b {
        PadButton::LeftTrigger => "SDL-0/+LeftTrigger".to_owned(),
        PadButton::RightTrigger => "SDL-0/+RightTrigger".to_owned(),
        b => format!("SDL-0/{}", sdl_name(*b)),
    };
    Some(h.chord.iter().map(part).collect::<Vec<_>>().join(" & "))
}

fn sdl_name(b: PadButton) -> &'static str {
    use PadButton::*;
    match b {
        South => "A",
        East => "B",
        North => "Y",
        West => "X",
        Back => "Back",
        Start => "Start",
        LeftShoulder => "LeftShoulder",
        RightShoulder => "RightShoulder",
        LeftTrigger => "LeftTrigger",
        RightTrigger => "RightTrigger",
        LeftStick => "LeftStick",
        RightStick => "RightStick",
        DPadUp => "DPadUp",
        DPadDown => "DPadDown",
        DPadLeft => "DPadLeft",
        DPadRight => "DPadRight",
    }
}

/// Write `emulator`'s own `[Hotkeys]` bindings for romforge's chords into
/// its settings file in `writes` (taken out when the hotkeys are off).
pub(super) fn bind(emulator: &str, on: bool, writes: &mut [ConfigWrite]) {
    let names = bound_names(emulator);
    let Some(w) = writes.first_mut().filter(|_| !names.is_empty()) else {
        return;
    };
    for (a, name) in names {
        let key = format!("[Hotkeys]{name}");
        match sdl_chord(*a).filter(|_| on) {
            Some(chord) => w.keys.push((key, chord)),
            None => w.remove.push(key),
        }
    }
}

/// A running emulator romforge sends commands to. Dropping it (the game
/// ended) stops any held command still being sent.
#[derive(Debug)]
pub struct Remote {
    channel: Channel,
    held: Arc<Held>,
}

/// Fast forward and rewind while held: set while their chord is down.
#[derive(Debug, Default)]
struct Held {
    fast: AtomicBool,
    rewind: AtomicBool,
}

/// How often a held command is sent again (more than once a frame).
const REPEAT: Duration = Duration::from_millis(10);

impl Remote {
    pub fn new(channel: Channel) -> Self {
        Remote {
            channel,
            held: Arc::default(),
        }
    }

    /// Start `a` (a held one until [`Remote::stop`]).
    pub fn start(&self, a: Action) -> std::io::Result<()> {
        let Channel::RetroArch { port } = self.channel;
        let (pick, command): (fn(&Held) -> &AtomicBool, _) = match a {
            Action::Quit => {
                // Twice, a moment apart: RetroArch asks to confirm a quit
                // and counts one command per frame.
                send(port, "QUIT")?;
                std::thread::sleep(Duration::from_millis(100));
                return send(port, "QUIT");
            }
            Action::SaveState => return send(port, "SAVE_STATE"),
            Action::LoadState => return send(port, "LOAD_STATE"),
            Action::Menu => return send(port, "MENU_TOGGLE"),
            Action::FastForward => (|h| &h.fast, "FAST_FORWARD_HOLD"),
            Action::Rewind => (|h| &h.rewind, "REWIND"),
        };
        if pick(&self.held).swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let held = Arc::downgrade(&self.held);
        std::thread::spawn(move || {
            while let Some(h) = held.upgrade() {
                if !pick(&h).load(Ordering::SeqCst) || send(port, command).is_err() {
                    pick(&h).store(false, Ordering::SeqCst);
                    break;
                }
                drop(h);
                std::thread::sleep(REPEAT);
            }
        });
        Ok(())
    }

    /// A held action's chord was let go.
    pub fn stop(&self, a: Action) {
        match a {
            Action::FastForward => self.held.fast.store(false, Ordering::SeqCst),
            Action::Rewind => self.held.rewind.store(false, Ordering::SeqCst),
            _ => {}
        }
    }
}

fn send(port: u16, command: &str) -> std::io::Result<()> {
    let s = UdpSocket::bind(("127.0.0.1", 0))?;
    s.send_to(command.as_bytes(), ("127.0.0.1", port))?;
    Ok(())
}

#[cfg(test)]
mod tests;
