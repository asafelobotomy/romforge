//! The hotkeys as the app shows them: the Hotkeys tab, the radial ring
//! and the short hints, all from the one table ([`HOTKEYS`]).

use super::{Action, HOTKEYS, RADIAL, RADIAL_BUTTON, RADIAL_HOLD};
use crate::launch::controls::{self, Route};

/// What an emulator does of the hotkeys: Quit for every emulator
/// (romforge asks it to quit), the rest where `launch::controls` can make
/// it do them.
pub fn support(emulator: &str) -> Vec<Action> {
    HOTKEYS
        .iter()
        .map(|h| h.action)
        .filter(|a| controls::route(emulator, *a) != Route::None)
        .collect()
}

/// What the radial ring can do in `emulator`: the actions romforge does
/// itself (a command, or a quit). One the emulator's own binding does
/// reacts to the chord on the pad, not to the ring, so it is left out.
pub fn ring(emulator: &str) -> Vec<Action> {
    RADIAL
        .iter()
        .copied()
        .filter(|a| matches!(controls::route(emulator, *a), Route::Command | Route::Stop))
        .collect()
}

/// The hotkeys for the Hotkeys tab: each chord as button labels, the
/// radial menu's ring, and what each of `emulators` gets.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HotkeysView {
    pub hotkeys: Vec<HotkeyRow>,
    pub radial: Vec<RadialSlot>,
    pub radial_hold_secs: u64,
    /// The button held for the radial menu ("L3").
    pub radial_button: &'static str,
    /// Quit's chord, as one label ("Select+Start").
    pub quit_chord: String,
    /// A one-line reminder for while a game runs.
    pub hint: String,
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
    /// What the radial ring can do in it ([`ring`]).
    pub ring: Vec<Action>,
    pub note: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HotkeyRow {
    pub action: Action,
    pub label: &'static str,
    pub chord: Vec<&'static str>,
    pub hold: bool,
}

/// `a`'s chord as one label ("Select+Start").
pub fn chord_label(a: Action) -> String {
    HOTKEYS
        .iter()
        .find(|h| h.action == a)
        .map(|h| h.chord.iter().map(|b| b.label()).collect::<Vec<_>>().join("+"))
        .unwrap_or_default()
}

pub fn view<'a>(emulators: impl IntoIterator<Item = &'a str>) -> HotkeysView {
    let label = |a: Action| HOTKEYS.iter().find(|h| h.action == a).map_or("", |h| h.label);
    let quit_chord = chord_label(Action::Quit);
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
        radial_button: RADIAL_BUTTON.label(),
        hint: format!("{quit_chord} quits · hold {} for the menu", RADIAL_BUTTON.label()),
        quit_chord,
        support: emulators
            .into_iter()
            .map(|e| SupportRow {
                emulator: e.to_owned(),
                actions: support(e),
                ring: ring(e),
                note: controls::note(e),
            })
            .collect(),
    }
}
