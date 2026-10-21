//! Milestone 18, stage 2, step 1: game pads. Read in Rust (gilrs, behind the
//! `controller` feature: `controller/pad.rs`), not through WebKitGTK's
//! Gamepad API, which only feeds the focused, visible window: romforge has
//! to hear the pad while an emulator has the focus too.
//!
//! This file is what doesn't need a pad: which pads to listen to (Steam's
//! duplicates), buttons to [`Action`]s, and a held direction repeating like
//! a held key.

use std::time::{Duration, Instant};

/// What a press means to romforge's UI. Named after the face buttons'
/// places (South is A on an Xbox pad, Cross on a PlayStation one), as SDL's
/// controller database names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    /// South: choose.
    Accept,
    /// East: back out.
    Back,
    /// North: per-game options.
    North,
    /// West.
    West,
    /// Start: the main menu.
    Menu,
    /// Select: a context menu.
    Context,
    /// Left shoulder: a page back.
    PageUp,
    /// Right shoulder: a page on.
    PageDown,
    /// Left trigger: to the first.
    First,
    /// Right trigger: to the last.
    Last,
}

/// What the app hears from the pads.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PadEvent {
    Action {
        action: Action,
    },
    Connected {
        name: String,
    },
    Disconnected {
        name: String,
    },
    /// A button going down or up, as it is (for hotkey chords,
    /// [`crate::hotkeys::Chords`]).
    Button {
        button: crate::hotkeys::PadButton,
        down: bool,
    },
    /// The right stick's position (x right, y up; -1..1).
    Stick {
        x: f32,
        y: f32,
    },
    /// Time passing (about every 30 ms), for what waits on a hold.
    Tick,
}

/// Steam Input's virtual pad.
pub const STEAM_PAD: (u16, u16) = (0x28de, 0x11ff);

/// A pad as far as choosing pads goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadId {
    pub id: usize,
    pub vendor: Option<u16>,
    pub product: Option<u16>,
}

/// `vendor/product` pairs in `SDL_GAMECONTROLLER_IGNORE_DEVICES`'s form
/// (`0x045e/0x028e,0x054c/0x0ce6`), which Steam sets for its games so they
/// see only its virtual pads.
pub fn parse_ignore(list: &str) -> Vec<(u16, u16)> {
    let hex =
        |s: &str| u16::from_str_radix(s.trim().trim_start_matches("0x").trim_start_matches("0X"), 16).ok();
    list.split(',')
        .filter_map(|pair| {
            let (v, p) = pair.split_once('/')?;
            Some((hex(v)?, hex(p)?))
        })
        .collect()
}

/// Which of `pads` to listen to: when Steam's virtual pad is there, only
/// Steam's (each physical pad shows up twice otherwise); else every pad
/// not in `ignore`.
pub fn usable(pads: &[PadId], ignore: &[(u16, u16)]) -> Vec<usize> {
    let ids = |p: &PadId| p.vendor.zip(p.product);
    let steam: Vec<usize> = pads
        .iter()
        .filter(|p| ids(p) == Some(STEAM_PAD))
        .map(|p| p.id)
        .collect();
    if !steam.is_empty() {
        return steam;
    }
    pads.iter()
        .filter(|p| !ids(p).is_some_and(|i| ignore.contains(&i)))
        .map(|p| p.id)
        .collect()
}

/// A button's action ([`pad`] maps gilrs's buttons to these names).
pub fn action_of(button: &str) -> Option<Action> {
    Some(match button {
        "DPadUp" => Action::Up,
        "DPadDown" => Action::Down,
        "DPadLeft" => Action::Left,
        "DPadRight" => Action::Right,
        "South" => Action::Accept,
        "East" => Action::Back,
        "North" => Action::North,
        "West" => Action::West,
        "Start" => Action::Menu,
        "Select" => Action::Context,
        "LeftTrigger" => Action::PageUp,
        "RightTrigger" => Action::PageDown,
        "LeftTrigger2" => Action::First,
        "RightTrigger2" => Action::Last,
        _ => return None,
    })
}

/// How far a stick goes before it counts as a direction, and how far back
/// before it lets go (so it doesn't flicker at the edge).
const STICK_ON: f32 = 0.6;
const STICK_OFF: f32 = 0.35;
/// A held direction repeats after this, then this often.
const FIRST_REPEAT: Duration = Duration::from_millis(400);
const REPEAT: Duration = Duration::from_millis(110);

/// Directions held down (d-pad or stick), repeating while held.
#[derive(Debug, Default)]
pub struct Directions {
    held: Option<(Action, Instant)>,
    stick_x: Option<Action>,
    stick_y: Option<Action>,
}

impl Directions {
    /// A direction pressed: its action at once, repeats from then on.
    pub fn press(&mut self, dir: Action, now: Instant) -> Action {
        self.held = Some((dir, now + FIRST_REPEAT));
        dir
    }

    /// A direction let go: no more repeats of it.
    pub fn release(&mut self, dir: Action) {
        if self.held.is_some_and(|(d, _)| d == dir) {
            self.held = None;
        }
    }

    /// A stick axis moved (`x` for left/right): a direction pressed or let
    /// go, with some slack between the two.
    pub fn stick(&mut self, x: bool, value: f32, now: Instant) -> Option<Action> {
        let (neg, pos) = if x {
            (Action::Left, Action::Right)
        } else {
            (Action::Down, Action::Up)
        };
        let slot = if x { self.stick_x } else { self.stick_y };
        let want = match slot {
            Some(_) if value.abs() < STICK_OFF => None,
            Some(d) => Some(d),
            None if value <= -STICK_ON => Some(neg),
            None if value >= STICK_ON => Some(pos),
            None => None,
        };
        if want == slot {
            return None;
        }
        if let Some(old) = slot {
            self.release(old);
        }
        if x {
            self.stick_x = want;
        } else {
            self.stick_y = want;
        }
        want.map(|d| self.press(d, now))
    }

    /// The held direction again, if it is due.
    pub fn tick(&mut self, now: Instant) -> Option<Action> {
        let (dir, next) = self.held?;
        if now < next {
            return None;
        }
        self.held = Some((dir, now + REPEAT));
        Some(dir)
    }

    /// Whether `a` is a direction.
    pub fn is_direction(a: Action) -> bool {
        matches!(a, Action::Up | Action::Down | Action::Left | Action::Right)
    }
}

#[cfg(feature = "controller")]
mod pad;
#[cfg(feature = "controller")]
pub use pad::listen;

#[cfg(test)]
mod tests;
