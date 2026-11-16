//! Milestone 27: save states (an emulator's snapshot of the whole
//! machine, in slots) beside the in-game saves, in
//! `Saves/<system>/states/`. Until romforge 0.18 they went to
//! `States/<system>/`: while that folder still holds states, emulators
//! keep writing there ([`dir`]), and [`merge`] moves them in (a plan
//! first, journaled for undo).
//!
//! Listed read-only, each matched to a game like a save
//! ([`super::index::match_all`]: serial in its name, else the content's
//! name), with its slot and the screenshot the emulator wrote beside it.

use std::path::{Path, PathBuf};

use super::index::{Save, serial_in, unfinished};
use crate::home::Home;

pub mod merge;

/// The folder in `Saves/<system>/` that holds its save states.
pub const STATES: &str = "states";

/// One save state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct State {
    /// Its system, file (below its states folder), size, time and game.
    #[serde(flatten)]
    pub save: Save,
    /// Its slot as the emulator names it ("1", "auto"), when its name says.
    pub slot: Option<String>,
    /// The screenshot the emulator wrote beside it, if any.
    pub picture: Option<PathBuf>,
    /// Still in the old `States/<system>/` folder.
    pub old_place: bool,
}

/// Where `system`'s emulators write save states: its old
/// `States/<system>/` while that holds a file (nothing there is lost or
/// hidden), else `Saves/<system>/states/`.
pub fn dir(home: &Home, system: &str) -> PathBuf {
    let old = home.states().join(system);
    if has_files(&old) {
        old
    } else {
        home.saves().join(system).join(STATES)
    }
}

/// Whether `dir` holds any file, at any depth.
pub(crate) fn has_files(dir: &Path) -> bool {
    walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .any(|e| e.file_type().is_file())
}

/// Every save state: each system's `Saves/<system>/states/`, then what is
/// still in the old `States/<system>/`. By system, then file.
pub fn walk(home: &Home) -> Vec<State> {
    let mut out = Vec::new();
    for system in system_folders(home) {
        out.extend(walk_system(home, &system));
    }
    out
}

/// The save states of one system (its folder named by its key, any case).
pub fn walk_system(home: &Home, system: &str) -> Vec<State> {
    let mut out = Vec::new();
    for (root, old_place) in [(home.saves(), false), (home.states(), true)] {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for e in entries.filter_map(Result::ok) {
            let Some(name) = e.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !name.eq_ignore_ascii_case(system) || !e.path().is_dir() {
                continue;
            }
            let dir = if old_place {
                e.path()
            } else {
                e.path().join(STATES)
            };
            in_folder(&dir, &name, old_place, &mut out);
        }
    }
    out.sort_by(|a, b| (&a.save.system, &a.save.file).cmp(&(&b.save.system, &b.save.file)));
    out
}

/// The system folders in `Saves/` and the old `States/`, once each.
fn system_folders(home: &Home) -> Vec<String> {
    let mut names: Vec<String> = [home.saves(), home.states()]
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flat_map(|it| it.filter_map(Result::ok))
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    names
}

/// The states in `dir` (one system's), pictures beside them attached.
fn in_folder(dir: &Path, system: &str, old_place: bool, out: &mut Vec<State>) {
    let files: Vec<walkdir::DirEntry> = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .collect();
    for e in &files {
        let name = e.file_name().to_string_lossy();
        if unfinished(&name) || is_picture(&name) || not_a_state(&name) {
            continue;
        }
        let Ok(meta) = e.metadata() else { continue };
        let rel = e.path().strip_prefix(dir).unwrap_or(e.path()).to_path_buf();
        out.push(State {
            slot: slot(&name),
            picture: picture_of(e.path()),
            old_place,
            save: Save {
                system: system.to_owned(),
                serial: serial_in(&rel.to_string_lossy()),
                file: rel,
                path: e.path().to_path_buf(),
                size: meta.len(),
                modified: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs()),
                game: None,
                matched_by: None,
            },
        });
    }
}

/// An emulator's spare copy, not a state to list: DuckStation's `.bak`,
/// PCSX2's `.backup`, Flycast's `.tmp` and `.net`, PPSSPP's undo states,
/// Dolphin's `.dtm` input recordings.
fn not_a_state(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".bak", ".backup", ".tmp", ".net", ".dtm"]
        .iter()
        .any(|x| lower.ends_with(x))
        || lower.contains(".undo.")
        || lower.starts_with("load_undo.")
}

/// A screenshot, never a state of its own.
fn is_picture(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".png") || lower.ends_with(".jpg")
}

/// The screenshot written beside `state`: `<state>.png` (RetroArch), else
/// the same name with `.png` or `.jpg` (PPSSPP).
fn picture_of(state: &Path) -> Option<PathBuf> {
    let mut beside = state.as_os_str().to_owned();
    beside.push(".png");
    [
        PathBuf::from(beside),
        state.with_extension("png"),
        state.with_extension("jpg"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// The slot a state's file name gives, in the emulator's own numbering:
/// RetroArch `.state` (0), `.state3`, `.state.auto`; mGBA `.ss1`, melonDS
/// `.ml1`, Mednafen `.mc1` and `.mca`, Dolphin `.s01`; DuckStation
/// `_1.sav` and `_resume.sav`, PPSSPP `_1.ppst`; PCSX2 `.01.p2s` and
/// `.resume.p2s`.
pub fn slot(name: &str) -> Option<String> {
    let digits = |s: &str| (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())).then(|| trim(s));
    if let Some(i) = name.rfind(".state") {
        return match &name[i + ".state".len()..] {
            "" => Some("0".into()),
            ".auto" => Some("auto".into()),
            rest => digits(rest),
        };
    }
    let (stem, ext) = name.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    match ext.as_str() {
        "sav" | "ppst" => {
            let (_, last) = stem.rsplit_once('_')?;
            if last.eq_ignore_ascii_case("resume") {
                Some("auto".into())
            } else {
                digits(last)
            }
        }
        "p2s" => match stem.rsplit_once('.')?.1 {
            "resume" => Some("auto".into()),
            n => digits(n),
        },
        "mca" => Some("auto".into()),
        _ => ["ss", "ml", "mc", "s"]
            .iter()
            .find_map(|p| ext.strip_prefix(p).and_then(digits)),
    }
}

/// A slot number without its leading zeros ("01" -> "1", "0" stays).
fn trim(s: &str) -> String {
    let t = s.trim_start_matches('0');
    if t.is_empty() { "0".into() } else { t.into() }
}

#[cfg(test)]
mod tests;
