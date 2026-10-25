//! Milestone 27, stage 1: in-game saves (memory cards, battery saves, save
//! folders; not save states) in `RomForge/Saves/<system>/`, set on every
//! start after the emulator's config ([`super::owned`]). The keys were
//! read in each emulator's source at the release named:
//!
//! - DuckStation (master 84e1db5): `[MemoryCards] Directory`, absolute
//!   (its cards are per game title by default, `<title>_1.mcd`).
//! - PCSX2 2.8.2: `[Folders] MemoryCards`, absolute.
//! - mGBA 0.10.5: `[ports.qt] savegamePath` (every core: GB and GBC too);
//!   unset, the save is beside the ROM.
//! - melonDS 1.1: `[Instance0] SaveFilePath`, read per instance; empty,
//!   beside the ROM.
//! - Mednafen 1.32.1: `filesys.path_sav` (default `sav`), files
//!   `<rom name>.<md5>.<ext>`.
//! - PPSSPP 1.20.4: no key on Linux (the memory stick is
//!   `$XDG_CONFIG_HOME/ppsspp`), so its `PSP/SAVEDATA/` is linked to
//!   `Saves/PSP/`.
//! - Dolphin 2609: `[Core] GCIFolderAPath` (it adds `/<region>` itself,
//!   no `Card A`) and `[Core] MemcardAPath` (it puts the region before the
//!   extension); always `Saves/GameCube/`, a Wii game's GameCube card too.
//!   The Wii NAND stays where it is for now (it also holds the Wii Menu).
//! - Flycast 2.7: `[config] Dreamcast.VMUPath` (Dreamcast) and
//!   `Dreamcast.SavePath` (Naomi and Atomiswave flash). A disc without a
//!   game ID still writes its VMU in Flycast's data folder.
//! - ScummVM 2026.4.0: `[scummvm] savepath`.
//! - MAME 0289: `nvram_directory` (default `nvram`, from the working
//!   folder).
//!
//! Nothing is lost by the switch: while the place it saved until now holds
//! a save (the emulator's folder in romforge's config, or for mGBA,
//! melonDS and Mednafen this game's save), that place stays. A key the
//! user set in Emulator Settings is theirs and stays as they set it.

use std::path::{Path, PathBuf};

use super::existing::UserDirs;
use super::formats::toml_string;
use super::{ConfigWrite, Format, Prepare};
use crate::home::Home;
use crate::systems::System;

/// One key romforge sets for the saves.
struct Key {
    file: PathBuf,
    format: Format,
    key: &'static str,
    value: String,
}

/// Point `name`'s saves at `Saves/<system>/` in `p` (its config in `dir`),
/// unless its saves are still where it kept them until now.
pub(crate) fn point(
    name: &str,
    home: &Home,
    system: &System,
    rom: &Path,
    dir: &Path,
    user: Option<&UserDirs>,
    p: &mut Prepare,
) {
    let to = home.saves().join(system.key);
    let s = |p: &Path| p.to_string_lossy().into_owned();
    let key = |file: &str, format, key, value| Key {
        file: dir.join(file),
        format,
        key,
        value,
    };
    let beside_rom = rom.with_extension("sav").is_file();
    let (keys, kept, to) = match name {
        "duckstation" => (
            vec![key(
                "duckstation/settings.ini",
                Format::Ini,
                "[MemoryCards]Directory",
                s(&to),
            )],
            holds_file(&dir.join("duckstation/memcards")),
            to,
        ),
        "pcsx2" => (
            vec![key(
                "PCSX2/inis/PCSX2.ini",
                Format::Ini,
                "[Folders]MemoryCards",
                s(&to),
            )],
            holds_file(&dir.join("PCSX2/memcards")),
            to,
        ),
        "mgba" => (
            vec![key(
                "mgba/config.ini",
                Format::Ini,
                "[ports.qt]savegamePath",
                s(&to),
            )],
            beside_rom,
            to,
        ),
        "melonds" => (
            vec![key(
                "melonDS/melonDS.toml",
                Format::Toml,
                "[Instance0]SaveFilePath",
                toml_string(&s(&to)),
            )],
            beside_rom,
            to,
        ),
        "mednafen" => {
            let stem = rom.file_stem().map(|f| format!("{}.", f.to_string_lossy()));
            let kept = stem.is_some_and(|st| any_file(&dir.join("sav"), |n| n.starts_with(&st)));
            (
                vec![key("mednafen.cfg", Format::Spaced, "filesys.path_sav", s(&to))],
                kept,
                to,
            )
        }
        "ppsspp" => {
            let savedata = dir.join("ppsspp/PSP/SAVEDATA");
            if !holds_file(&savedata) {
                p.folder_links.push((to.clone(), savedata));
            }
            (Vec::new(), false, to)
        }
        "dolphin" => {
            let to = home.saves().join("GameCube");
            let gc = dir.join("GC");
            let kept = any_file(&gc, |n| n.starts_with("MemoryCard"))
                || std::fs::read_dir(&gc)
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .any(|r| holds_file(&r.path().join("Card A")));
            let keys = vec![
                key("Config/Dolphin.ini", Format::Ini, "[Core]GCIFolderAPath", s(&to)),
                key(
                    "Config/Dolphin.ini",
                    Format::Ini,
                    "[Core]MemcardAPath",
                    s(&to.join("MemoryCardA.raw")),
                ),
            ];
            (keys, kept, to)
        }
        "flycast" => {
            let data = dir.join("data/flycast");
            if system.key == "Dreamcast" {
                let kept = any_file(&data, |n| n.contains("vmu_save"));
                (
                    vec![key(
                        "flycast/emu.cfg",
                        Format::Ini,
                        "[config]Dreamcast.VMUPath",
                        s(&to),
                    )],
                    kept,
                    to,
                )
            } else {
                let file = rom.file_name().map(|f| f.to_string_lossy().into_owned());
                let kept = file.is_some_and(|f| any_file(&data, |n| n.starts_with(&f)));
                (
                    vec![key(
                        "flycast/emu.cfg",
                        Format::Ini,
                        "[config]Dreamcast.SavePath",
                        s(&to),
                    )],
                    kept,
                    to,
                )
            }
        }
        "scummvm" => {
            let elsewhere = user.is_some_and(|u| holds_file(&u.xdg.data.join("scummvm/saves")));
            let k = key("scummvm.ini", Format::Ini, "[scummvm]savepath", s(&to));
            let kept = elsewhere || set_elsewhere(&k, p, |v| v != k.value);
            (vec![k], kept, to)
        }
        "mame" => {
            let k = key(
                "mame.ini",
                Format::Spaced,
                "nvram_directory",
                format!("\"{}\"", s(&to)),
            );
            let kept = set_elsewhere(&k, p, |v| v.trim_matches('"') != s(&to) && v != "nvram");
            (vec![k], kept, to)
        }
        _ => return,
    };
    p.dirs.push(to);
    for k in keys {
        set(p, k, kept);
    }
}

/// Whether `k` already has a value (in romforge's copy of the file, else
/// the user's own it will be seeded from) that `other` calls another
/// place the user keeps their saves.
fn set_elsewhere(k: &Key, p: &Prepare, other: impl Fn(&str) -> bool) -> bool {
    let from = if k.file.exists() {
        Some(k.file.clone())
    } else {
        p.seeds
            .iter()
            .find(|(_, ours)| *ours == k.file)
            .map(|(theirs, _)| theirs.clone())
    };
    let text = from
        .and_then(|f| std::fs::read_to_string(f).ok())
        .unwrap_or_default();
    super::read(k.format, &text)
        .into_iter()
        .any(|(name, v)| name.eq_ignore_ascii_case(k.key) && other(&v))
}

/// Set `k` in `p`'s write of its file (`kept`: take it out, so the
/// emulator saves where it did); one set by the user is left as it is.
fn set(p: &mut Prepare, k: Key, kept: bool) {
    let w = match p.writes.iter_mut().position(|w| w.path == k.file) {
        Some(i) => &mut p.writes[i],
        None => {
            p.writes.push(ConfigWrite {
                path: k.file.clone(),
                format: k.format,
                keys: Vec::new(),
                remove: Vec::new(),
            });
            p.writes.last_mut().expect("just pushed")
        }
    };
    let theirs = w.keys.iter().any(|(n, _)| n == k.key) || w.remove.iter().any(|n| n == k.key);
    if theirs {
        return;
    }
    if kept {
        w.remove.push(k.key.to_owned());
    } else {
        w.keys.push((k.key.to_owned(), k.value));
    }
}

/// Whether there is a file anywhere under `dir`.
fn holds_file(dir: &Path) -> bool {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .any(|e| e.file_type().is_file())
}

/// Whether `dir` holds a file (directly) whose name passes `test`.
fn any_file(dir: &Path, test: impl Fn(&str) -> bool) -> bool {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|e| e.file_type().is_ok_and(|t| t.is_file()) && test(&e.file_name().to_string_lossy()))
}

#[cfg(test)]
mod tests;
