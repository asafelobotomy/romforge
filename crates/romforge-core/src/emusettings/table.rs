//! The one table of Emulator Settings (house rule 4). A setting is applied
//! when romforge starts an emulator (fullscreen, program, arguments), or
//! written into romforge's own copy of the emulator's config, under the
//! keys listed for it (`keys`), each in that emulator's own words
//! ([`Words`]); an emulator without such a key doesn't show the setting.
//! Every key was read from the emulator's source at the release its entry
//! names (milestone 21, 2026-10-25), never written from memory.

use crate::launch::{Format, KnownEmulator};

mod files;
mod launch;
mod renderer;
mod video;

/// What a setting holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// `true` or `false`.
    Bool,
    /// One of `choices`.
    Choice,
    /// A program on this computer.
    Path,
    /// Command-line arguments, split at spaces.
    Args,
}

/// How romforge's value is written in one emulator's config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "as", content = "with", rename_all = "lowercase")]
pub enum Words {
    /// As it is.
    Same,
    /// `true` / `false` written as these two.
    Bool(&'static str, &'static str),
    /// Each of romforge's values that the emulator has, as it writes it; a
    /// value it lacks leaves the key out (its own default applies).
    Map(&'static [(&'static str, &'static str)]),
    /// A whole number times this (Flycast's resolution is 480 × the scale).
    Times(u32),
}

impl Words {
    /// `v` in the emulator's words; `None`: leave the key out.
    pub fn of(&self, v: &str) -> Option<String> {
        match self {
            Words::Same => Some(v.to_owned()),
            Words::Bool(on, off) => match v {
                "true" => Some((*on).to_owned()),
                "false" => Some((*off).to_owned()),
                _ => None,
            },
            Words::Map(pairs) => pairs.iter().find(|(r, _)| *r == v).map(|(_, e)| (*e).to_owned()),
            Words::Times(n) => v.parse::<u32>().ok().map(|x| (x * n).to_string()),
        }
    }

    /// The values of romforge's it can write, for a choice (all: `None`).
    pub fn choices(&self) -> Option<Vec<&'static str>> {
        match self {
            Words::Map(pairs) => Some(pairs.iter().map(|(r, _)| *r).collect()),
            _ => None,
        }
    }
}

/// Where one emulator keeps a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Key {
    pub emulator: &'static str,
    /// Its file, from romforge's config folder for the emulator.
    pub file: &'static str,
    /// As the writer for `format` names it (`[Section]Key`, `Video/Renderer`).
    pub key: &'static str,
    pub format: Format,
    pub words: Words,
}

/// A key (table shorthand).
const fn k(
    emulator: &'static str,
    file: &'static str,
    key: &'static str,
    format: Format,
    words: Words,
) -> Key {
    Key {
        emulator,
        file,
        key,
        format,
        words,
    }
}

/// One setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Setting {
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub kind: Kind,
    /// It can be set for every emulator at once.
    pub global: bool,
    /// The value when nothing is set; empty: whatever the emulator's own
    /// config says.
    pub default: &'static str,
    /// For [`Kind::Choice`]: value and label.
    pub choices: &'static [(&'static str, &'static str)],
    /// Where it is written; every key listed for an emulator gets the value.
    pub keys: &'static [Key],
}

/// A yes-or-no setting written to config files (table shorthand).
const fn config(key: &'static str, label: &'static str, hint: &'static str, keys: &'static [Key]) -> Setting {
    Setting {
        key,
        label,
        hint,
        kind: Kind::Bool,
        global: true,
        default: "",
        choices: &[],
        keys,
    }
}

/// A choice written to config files (table shorthand).
const fn choice(
    key: &'static str,
    label: &'static str,
    hint: &'static str,
    choices: &'static [(&'static str, &'static str)],
    keys: &'static [Key],
) -> Setting {
    Setting {
        key,
        label,
        hint,
        kind: Kind::Choice,
        global: true,
        default: "",
        choices,
        keys,
    }
}

/// Parts of the table, one after the other.
const fn concat<T: Copy, const N: usize>(parts: &[&[T]]) -> [T; N] {
    let mut out = [parts[0][0]; N];
    let (mut p, mut n) = (0, 0);
    while p < parts.len() {
        let mut i = 0;
        while i < parts[p].len() {
            out[n] = parts[p][i];
            (i, n) = (i + 1, n + 1);
        }
        p += 1;
    }
    assert!(n == N);
    out
}

pub const SETTINGS: &[Setting] =
    &concat::<Setting, { launch::LAUNCH.len() + video::VIDEO.len() }>(&[launch::LAUNCH, video::VIDEO]);

pub(super) fn setting(key: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|s| s.key == key)
}

/// Whether `s` means anything for `e`.
pub(super) fn applies(s: &Setting, e: &KnownEmulator) -> bool {
    match s.key {
        "fullscreen" => !e.fullscreen.is_empty(),
        "program" | "args" => true,
        _ => s.keys.iter().any(|k| k.emulator == e.settings_name()),
    }
}

/// The choices of `s` that `e` has; `None`: all (one of its keys takes
/// any value).
pub(super) fn choices_for(s: &Setting, e: &KnownEmulator) -> Option<Vec<&'static str>> {
    let mut out: Option<Vec<&'static str>> = None;
    for k in s.keys.iter().filter(|k| k.emulator == e.settings_name()) {
        let c = k.words.choices()?;
        let all = out.get_or_insert_with(Vec::new);
        for v in c {
            if !all.contains(&v) {
                all.push(v);
            }
        }
    }
    out
}
