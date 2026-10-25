//! Where each emulator saved before `Saves/` (milestone 27): the one list,
//! read by the switch (`launch/owned/saves.rs`: while a save is still
//! here, the emulator keeps saving here) and by gathering
//! (`saves::gather`, which moves them into `Saves/`).

use crate::systems::key;
use std::path::{Path, PathBuf};

/// Folders in romforge's config for an emulator whose every file is a save
/// of one system: (emulator, folder, system key).
const FOLDERS: &[(&str, &str, &str)] = &[
    ("duckstation", "duckstation/memcards", key!("PlayStation")),
    ("pcsx2", "PCSX2/memcards", key!("PlayStation 2")),
    ("ppsspp", PPSSPP_SAVEDATA, key!("PSP")),
];
/// PPSSPP's `SAVEDATA`, linked to `Saves/PSP/` once it holds no save.
pub(crate) const PPSSPP_SAVEDATA: &str = "ppsspp/PSP/SAVEDATA";
/// Dolphin's GameCube folder: `<region>/Card A/` (GCI folders) and
/// `MemoryCardA.<region>.raw`, to `Saves/GameCube/`.
const DOLPHIN_GC: &str = "GC";
const DOLPHIN_CARD: &str = "Card A";
const DOLPHIN_RAW: &str = "MemoryCardA.";
/// Flycast's data folder; its Dreamcast VMUs are `…vmu_save…` in it.
pub(crate) const FLYCAST_DATA: &str = "data/flycast";
const FLYCAST_VMU: &str = "vmu_save";
/// Mednafen's `<game>.<md5>.<ext>`, matched to a system by its game.
pub(crate) const MEDNAFEN_SAV: &str = "sav";
/// ScummVM's own save folder, in your XDG data folder (outside RomForge).
pub(crate) const SCUMMVM_SAVES: &str = "scummvm/saves";

/// Every file under `dir`, with its path below it.
pub(crate) fn files_under(dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            let rel = e.path().strip_prefix(dir).unwrap_or(e.path()).to_path_buf();
            (e.path().to_path_buf(), rel)
        })
        .collect()
}

/// The saves in `name`'s old places inside its config folder `dir` whose
/// system the place tells, each with where it goes below `Saves/`
/// (`<system key>/…`). Mednafen's, mGBA's and melonDS's go by their game
/// instead (`saves::gather`).
pub(crate) fn in_config(name: &str, dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    if let Some((_, folder, system)) = FOLDERS.iter().find(|(e, _, _)| *e == name) {
        let folder = dir.join(folder);
        if folder.is_symlink() {
            return Vec::new();
        }
        return files_under(&folder)
            .into_iter()
            .map(|(f, rel)| (f, Path::new(system).join(rel)))
            .collect();
    }
    match name {
        "dolphin" => {
            let to = Path::new(key!("GameCube"));
            let mut out = Vec::new();
            for e in std::fs::read_dir(dir.join(DOLPHIN_GC))
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
            {
                let n = e.file_name().to_string_lossy().into_owned();
                if e.path().is_dir() {
                    out.extend(
                        files_under(&e.path().join(DOLPHIN_CARD))
                            .into_iter()
                            .map(|(f, rel)| (f, to.join(&n).join(rel))),
                    );
                } else if n.starts_with(DOLPHIN_RAW) {
                    out.push((e.path(), to.join(&n)));
                }
            }
            out
        }
        "flycast" => files_under(&dir.join(FLYCAST_DATA))
            .into_iter()
            .filter(|(_, rel)| {
                rel.parent() == Some(Path::new("")) && rel.to_string_lossy().contains(FLYCAST_VMU)
            })
            .map(|(f, rel)| (f, Path::new(key!("Dreamcast")).join(rel)))
            .collect(),
        _ => Vec::new(),
    }
}
