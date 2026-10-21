//! Stage 2, step 3: romforge's hotkeys, the one table of them (house rule
//! 4). romforge is the controller layer for every emulator: it reads these
//! chords on the pad itself while a game runs (it hears the pad even when
//! the emulator has the focus) and makes the emulator do the action
//! (`launch::controls`); emulators' own pad hotkeys are off.
//!
//! Select is the hotkey button. Fast forward and rewind last while their
//! chord is held. Holding L3 alone for [`RADIAL_HOLD`] opens a ring of
//! actions ([`RADIAL`]) the right stick picks from; letting go of L3 does
//! the one it points at.
//!
//! [`Chords`] is a pure state machine (no pad, no clock of its own), so it
//! is tested without one.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

/// A pad button, by SDL GameController's names (South is A on an Xbox pad,
/// Cross on a PlayStation one; Back is Select).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PadButton {
    South,
    East,
    North,
    West,
    Back,
    Start,
    LeftShoulder,
    RightShoulder,
    LeftTrigger,
    RightTrigger,
    LeftStick,
    RightStick,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

impl PadButton {
    /// A gilrs button (by its `Debug` name) as SDL names it: gilrs calls
    /// the shoulders `LeftTrigger`/`RightTrigger` and the triggers
    /// `LeftTrigger2`/`RightTrigger2`. The guide button (`Mode`) is left
    /// to Steam and the system.
    pub fn from_gilrs(name: &str) -> Option<PadButton> {
        use PadButton::*;
        Some(match name {
            "South" => South,
            "East" => East,
            "North" => North,
            "West" => West,
            "Select" => Back,
            "Start" => Start,
            "LeftTrigger" => LeftShoulder,
            "RightTrigger" => RightShoulder,
            "LeftTrigger2" => LeftTrigger,
            "RightTrigger2" => RightTrigger,
            "LeftThumb" => LeftStick,
            "RightThumb" => RightStick,
            "DPadUp" => DPadUp,
            "DPadDown" => DPadDown,
            "DPadLeft" => DPadLeft,
            "DPadRight" => DPadRight,
            _ => return None,
        })
    }

    /// How the Hotkeys tab shows it.
    pub fn label(self) -> &'static str {
        use PadButton::*;
        match self {
            South => "A",
            East => "B",
            North => "Y",
            West => "X",
            Back => "Select",
            Start => "Start",
            LeftShoulder => "L1",
            RightShoulder => "R1",
            LeftTrigger => "L2",
            RightTrigger => "R2",
            LeftStick => "L3",
            RightStick => "R3",
            DPadUp => "Up",
            DPadDown => "Down",
            DPadLeft => "Left",
            DPadRight => "Right",
        }
    }
}

/// What a hotkey does in the running game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Quit,
    SaveState,
    LoadState,
    /// The emulator's own menu (it pauses the game).
    Menu,
    FastForward,
    Rewind,
}

/// One hotkey: its chord, and whether it lasts while held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Hotkey {
    pub action: Action,
    pub chord: &'static [PadButton],
    /// Lasts while the chord is held (fast forward, rewind), else once.
    pub hold: bool,
    pub label: &'static str,
}

use PadButton::*;

/// The hotkeys (fixed for now; an editor comes later).
pub const HOTKEYS: &[Hotkey] = &[
    Hotkey {
        action: Action::Quit,
        chord: &[Back, Start],
        hold: false,
        label: "Quit",
    },
    Hotkey {
        action: Action::SaveState,
        chord: &[Back, RightShoulder],
        hold: false,
        label: "Save state",
    },
    Hotkey {
        action: Action::LoadState,
        chord: &[Back, LeftShoulder],
        hold: false,
        label: "Load state",
    },
    Hotkey {
        action: Action::Menu,
        chord: &[LeftStick, RightStick],
        hold: false,
        label: "Emulator menu",
    },
    Hotkey {
        action: Action::FastForward,
        chord: &[Back, RightTrigger],
        hold: true,
        label: "Fast forward (held)",
    },
    Hotkey {
        action: Action::Rewind,
        chord: &[Back, LeftTrigger],
        hold: true,
        label: "Rewind (held)",
    },
];

/// The radial menu's actions, clockwise from the top.
pub const RADIAL: &[Action] = &[
    Action::Quit,
    Action::SaveState,
    Action::FastForward,
    Action::Menu,
    Action::Rewind,
    Action::LoadState,
];

/// How long L3 is held alone before the radial menu opens.
pub const RADIAL_HOLD: Duration = Duration::from_secs(2);

/// What [`Chords`] makes of the pad.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HotkeyEvent {
    /// Do it (a held one until [`HotkeyEvent::Stop`]).
    Start {
        action: Action,
    },
    /// A held one's chord was let go.
    Stop {
        action: Action,
    },
    RadialOpen,
    /// The slot of [`RADIAL`] the right stick points at (none: the middle).
    RadialSlot {
        slot: Option<usize>,
    },
    RadialClose,
}

/// The pad's held buttons, the held hotkeys running, and the radial menu.
#[derive(Debug, Default)]
pub struct Chords {
    held: BTreeSet<PadButton>,
    running: Vec<Action>,
    l3_since: Option<Instant>,
    /// Open, with the slot pointed at.
    radial: Option<Option<usize>>,
}

impl Chords {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the radial menu is open (the game's other hotkeys wait).
    pub fn radial_open(&self) -> bool {
        self.radial.is_some()
    }

    pub fn press(&mut self, b: PadButton, now: Instant) -> Vec<HotkeyEvent> {
        self.held.insert(b);
        if let Some(slot) = self.radial {
            return match b {
                South => self.choose(slot),
                East => self.close(),
                _ => Vec::new(),
            };
        }
        self.l3_since = (b == LeftStick && self.held.len() == 1).then_some(now);
        // The chord this press completes; of several, the longest.
        let done = HOTKEYS
            .iter()
            .filter(|h| h.chord.contains(&b) && h.chord.iter().all(|c| self.held.contains(c)))
            .max_by_key(|h| h.chord.len());
        let Some(h) = done else { return Vec::new() };
        if h.hold {
            if self.running.contains(&h.action) {
                return Vec::new();
            }
            self.running.push(h.action);
        }
        vec![HotkeyEvent::Start { action: h.action }]
    }

    pub fn release(&mut self, b: PadButton) -> Vec<HotkeyEvent> {
        self.held.remove(&b);
        let mut out = Vec::new();
        if b == LeftStick {
            self.l3_since = None;
            if let Some(slot) = self.radial {
                out.extend(self.choose(slot));
            }
        }
        let held = &self.held;
        self.running.retain(|a| {
            let still = HOTKEYS
                .iter()
                .find(|h| h.action == *a)
                .is_some_and(|h| h.chord.iter().all(|c| held.contains(c)));
            if !still {
                out.push(HotkeyEvent::Stop { action: *a });
            }
            still
        });
        out
    }

    /// The right stick's position (x right, y up; -1..1).
    pub fn stick(&mut self, x: f32, y: f32) -> Vec<HotkeyEvent> {
        let Some(old) = self.radial else { return Vec::new() };
        let slot = slot_at(x, y, old);
        if slot == old {
            return Vec::new();
        }
        self.radial = Some(slot);
        vec![HotkeyEvent::RadialSlot { slot }]
    }

    /// Time passing: the radial menu opens once L3 has been held alone
    /// long enough.
    pub fn tick(&mut self, now: Instant) -> Vec<HotkeyEvent> {
        match self.l3_since {
            Some(t)
                if self.radial.is_none() && self.held.len() == 1 && now.duration_since(t) >= RADIAL_HOLD =>
            {
                self.radial = Some(None);
                vec![HotkeyEvent::RadialOpen]
            }
            _ => Vec::new(),
        }
    }

    /// Do the slot pointed at (none: just close).
    fn choose(&mut self, slot: Option<usize>) -> Vec<HotkeyEvent> {
        let mut out = self.close();
        if let Some(a) = slot.and_then(|s| RADIAL.get(s)) {
            out.push(HotkeyEvent::Start { action: *a });
        }
        out
    }

    fn close(&mut self) -> Vec<HotkeyEvent> {
        self.radial = None;
        self.l3_since = None;
        vec![HotkeyEvent::RadialClose]
    }
}

/// The ring slot a stick position points at: none near the middle, with a
/// little slack so a slot doesn't flicker as the stick comes back.
fn slot_at(x: f32, y: f32, was: Option<usize>) -> Option<usize> {
    let r = (x * x + y * y).sqrt();
    let edge = if was.is_some() { 0.35 } else { 0.6 };
    if r < edge {
        return None;
    }
    let n = RADIAL.len() as f32;
    // Clockwise from the top.
    let angle = x.atan2(y).rem_euclid(std::f32::consts::TAU);
    Some(((angle / std::f32::consts::TAU * n).round() as usize) % RADIAL.len())
}

/// What an emulator does of the hotkeys: Quit for every emulator
/// (romforge asks it to quit), the rest where `launch::controls` can make
/// it do them.
pub fn support(emulator: &str) -> Vec<Action> {
    HOTKEYS
        .iter()
        .map(|h| h.action)
        .filter(|a| crate::launch::controls::route(emulator, *a) != crate::launch::controls::Route::None)
        .collect()
}

/// The hotkeys for the Hotkeys tab: each chord as button labels, the
/// radial menu's ring, and what each of `emulators` gets.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HotkeysView {
    pub hotkeys: Vec<HotkeyRow>,
    pub radial: Vec<RadialSlot>,
    pub radial_hold_secs: u64,
    pub support: Vec<SupportRow>,
}

/// A slot of the radial menu's ring.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RadialSlot {
    pub action: Action,
    pub label: &'static str,
}

/// What one emulator gets.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SupportRow {
    pub emulator: String,
    pub actions: Vec<Action>,
    pub note: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HotkeyRow {
    pub action: Action,
    pub label: &'static str,
    pub chord: Vec<&'static str>,
    pub hold: bool,
}

pub fn view<'a>(emulators: impl IntoIterator<Item = &'a str>) -> HotkeysView {
    let label = |a: Action| HOTKEYS.iter().find(|h| h.action == a).map_or("", |h| h.label);
    HotkeysView {
        hotkeys: HOTKEYS
            .iter()
            .map(|h| HotkeyRow {
                action: h.action,
                label: h.label,
                chord: h.chord.iter().map(|b| b.label()).collect(),
                hold: h.hold,
            })
            .collect(),
        radial: RADIAL
            .iter()
            .map(|a| RadialSlot {
                action: *a,
                label: label(*a),
            })
            .collect(),
        radial_hold_secs: RADIAL_HOLD.as_secs(),
        support: emulators
            .into_iter()
            .map(|e| SupportRow {
                emulator: e.to_owned(),
                actions: support(e),
                note: crate::launch::controls::note(e),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests;
