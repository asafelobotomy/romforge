//! Milestone 27: save states in `Saves/<system>/states/`
//! ([`crate::saves::states::dir`]), set on every start beside the save
//! keys. Read in each emulator's source at the release named:
//!
//! - DuckStation (master 84e1db5): `[Folders] SaveStates` (default
//!   `<data root>/savestates`, `src/core/settings.cpp`).
//! - PCSX2 2.8.2: `[Folders] Savestates` (default `sstates`,
//!   `pcsx2/Pcsx2Config.cpp`).
//! - mGBA 0.10.5: `[ports.qt] savestatePath` (unset: beside the ROM,
//!   `src/core/directories.c`).
//! - melonDS 1.1: `[Instance0] SavestatePath` (empty: beside the ROM,
//!   `EmuInstance.cpp`).
//! - Mednafen: `filesys.path_state` (default `mcs`, `general.cpp`; read
//!   in the copy OpenEmu embeds, its version not stated).
//! - Flycast 2.7: `[config] Dreamcast.SavestatePath` (unset: its data
//!   folder, `core/cfg/option.cpp`).
//! - PPSSPP 1.20.4: no key; the memory stick's `PSP/PPSSPP_STATE` is
//!   linked to the states folder, as `SAVEDATA` is to `Saves/PSP/`.
//! - MAME 0289: `state_directory` (default `sta`, from the working
//!   folder); it adds a folder per driver below.
//!
//! Dolphin (2609) has no key (its `StateSaves/` moves only with its whole
//! user folder), so its states stay there. As with the saves, an
//! emulator whose old place still holds a state keeps it (the key taken
//! out), and a key the user set in Emulator Settings stays theirs.

use std::path::Path;

use super::{Key, any_file, holds_file, set, set_elsewhere};
use crate::home::Home;
use crate::launch::owned::formats::toml_string;
use crate::launch::owned::{Format, Prepare, files};
use crate::systems::System;

/// Point `name`'s save states at `system`'s states folder in `p` (its
/// config in `dir`), unless its states are still where it kept them.
pub(crate) fn point(name: &str, home: &Home, system: &System, rom: &Path, dir: &Path, p: &mut Prepare) {
    let to = crate::saves::states::dir(home, system.key);
    let s = to.to_string_lossy().into_owned();
    let key = |file: &str, format, key, value| Key {
        file: dir.join(file),
        format,
        key,
        value,
    };
    // A state beside the ROM: `<rom name>.<ext><slot>`.
    let beside_rom = |ext: &str| {
        let stem = rom.file_stem().map(|f| format!("{}.{ext}", f.to_string_lossy()));
        let folder = rom.parent().unwrap_or(Path::new(""));
        stem.is_some_and(|st| any_file(folder, |n| n.starts_with(&st)))
    };
    let (k, kept) = match name {
        "duckstation" => (
            key(files::DS, Format::Ini, "[Folders]SaveStates", s.clone()),
            holds_file(&dir.join("duckstation/savestates")),
        ),
        "pcsx2" => (
            key(files::PCSX2, Format::Ini, "[Folders]Savestates", s.clone()),
            holds_file(&dir.join("PCSX2/sstates")),
        ),
        "mgba" => (
            key(files::MGBA, Format::Ini, "[ports.qt]savestatePath", s.clone()),
            beside_rom("ss"),
        ),
        "melonds" => (
            key(
                files::MELONDS,
                Format::Toml,
                "[Instance0]SavestatePath",
                toml_string(&s),
            ),
            beside_rom("ml"),
        ),
        "mednafen" => {
            let stem = rom.file_stem().map(|f| format!("{}.", f.to_string_lossy()));
            let kept = stem.is_some_and(|st| any_file(&dir.join("mcs"), |n| n.starts_with(&st)));
            (
                key(files::MEDNAFEN, Format::Spaced, "filesys.path_state", s.clone()),
                kept,
            )
        }
        "flycast" => {
            let data = dir.join(super::old::FLYCAST_DATA);
            let kept = any_file(&data, |n| n.contains(".state"));
            (
                key(
                    files::FLYCAST,
                    Format::Ini,
                    "[config]Dreamcast.SavestatePath",
                    s.clone(),
                ),
                kept,
            )
        }
        "ppsspp" => {
            let states = dir.join(PPSSPP_STATE);
            if !holds_file(&states) {
                p.folder_links.push((to.clone(), states));
            }
            p.dirs.push(to);
            return;
        }
        "mame" => {
            let k = key(files::MAME, Format::Spaced, "state_directory", format!("\"{s}\""));
            let kept = set_elsewhere(&k, p, |v| v.trim_matches('"') != s && v != "sta");
            (k, kept)
        }
        _ => return,
    };
    p.dirs.push(to);
    set(p, k, kept);
}

/// PPSSPP's states folder on its memory stick, in romforge's config.
pub(crate) const PPSSPP_STATE: &str = "ppsspp/PSP/PPSSPP_STATE";

#[cfg(test)]
mod tests;
