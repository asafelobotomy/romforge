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
//! - DOSBox Staging and DOSBox-X: no key; an overlay ([`dos_overlay`]).
//!
//! Nothing is lost by the switch: while the place it saved until now holds
//! a save (the emulator's folder in romforge's config, or for mGBA,
//! melonDS and Mednafen this game's save), that place stays. A key the
//! user set in Emulator Settings is theirs and stays as they set it.

use std::path::{Path, PathBuf};

use super::existing::UserDirs;
use super::files;
use super::formats::toml_string;
use super::{ConfigWrite, Format, Prepare};
use crate::home::Home;
use crate::systems::System;
use crate::systems::key;

pub(crate) mod old;

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
            vec![key(files::DS, Format::Ini, "[MemoryCards]Directory", s(&to))],
            !old::in_config(name, dir).is_empty(),
            to,
        ),
        "pcsx2" => (
            vec![key(files::PCSX2, Format::Ini, "[Folders]MemoryCards", s(&to))],
            !old::in_config(name, dir).is_empty(),
            to,
        ),
        "mgba" => (
            vec![key(files::MGBA, Format::Ini, "[ports.qt]savegamePath", s(&to))],
            beside_rom,
            to,
        ),
        "melonds" => (
            vec![key(
                files::MELONDS,
                Format::Toml,
                "[Instance0]SaveFilePath",
                toml_string(&s(&to)),
            )],
            beside_rom,
            to,
        ),
        "mednafen" => {
            let stem = rom.file_stem().map(|f| format!("{}.", f.to_string_lossy()));
            let kept = stem.is_some_and(|st| any_file(&dir.join(old::MEDNAFEN_SAV), |n| n.starts_with(&st)));
            (
                vec![key(files::MEDNAFEN, Format::Spaced, "filesys.path_sav", s(&to))],
                kept,
                to,
            )
        }
        "ppsspp" => {
            let savedata = dir.join(old::PPSSPP_SAVEDATA);
            if old::in_config(name, dir).is_empty() {
                p.folder_links.push((to.clone(), savedata));
            }
            (Vec::new(), false, to)
        }
        "dolphin" => {
            let to = home.saves().join(key!("GameCube"));
            let kept = !old::in_config(name, dir).is_empty();
            let keys = vec![
                key(files::DOLPHIN, Format::Ini, "[Core]GCIFolderAPath", s(&to)),
                key(
                    files::DOLPHIN,
                    Format::Ini,
                    "[Core]MemcardAPath",
                    s(&to.join("MemoryCardA.raw")),
                ),
            ];
            (keys, kept, to)
        }
        "flycast" => {
            let data = dir.join(old::FLYCAST_DATA);
            if system.key == key!("Dreamcast") {
                let kept = !old::in_config(name, dir).is_empty();
                (
                    vec![key(
                        files::FLYCAST,
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
                        files::FLYCAST,
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
            let elsewhere = user.is_some_and(|u| holds_file(&u.xdg.data.join(old::SCUMMVM_SAVES)));
            let k = key(files::SCUMMVM, Format::Ini, "[scummvm]savepath", s(&to));
            let kept = elsewhere || set_elsewhere(&k, p, |v| v != k.value);
            (vec![k], kept, to)
        }
        "mame" => {
            let k = key(
                files::MAME,
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

/// DOSBox Staging (0.82.2) and DOSBox-X (2026.10.01) have no save folder:
/// a game writes into its own. So its folder is mounted as `C:` and
/// `Saves/DOS/<game>/` over it as an overlay (`mount C <dir> -t overlay`):
/// the game sees its files as they are, what it writes lands in the
/// overlay, and its folder is never written. `args` has the program where
/// `{rom}` was; it becomes the mounts and the program's name. Only for a
/// program in a folder (an archive is handed over as it is).
pub(crate) fn dos_overlay(
    name: &str,
    home: &Home,
    system: &System,
    game: &str,
    rom: &Path,
    args: &mut Vec<String>,
    p: &mut Prepare,
) {
    if !matches!(name, "dosbox-staging" | "dosbox-x") || !rom.is_file() {
        return;
    }
    let program = rom.file_name().map(|f| f.to_string_lossy().into_owned());
    let (Some(program), Some(folder)) = (program, rom.parent()) else {
        return;
    };
    let lower = program.to_ascii_lowercase();
    if program.contains(' ') || ![".exe", ".com", ".bat"].iter().any(|x| lower.ends_with(x)) {
        return;
    }
    let Some(at) = args.iter().position(|a| Path::new(a) == rom) else {
        return;
    };
    let overlay = home
        .saves()
        .join(system.key)
        .join(crate::organize::sanitize(game));
    let s = |p: &Path| p.to_string_lossy().into_owned();
    let cmds = [
        format!("mount C \"{}\"", s(folder)),
        format!("mount C \"{}\" -t overlay", s(&overlay)),
        "C:".to_owned(),
        program,
    ];
    args.splice(at..=at, cmds.into_iter().flat_map(|c| ["-c".to_owned(), c]));
    p.dirs.push(overlay);
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
