//! Milestone 27, stage 1: the emulators whose saves can't be pointed at
//! `Saves/` and stay inside a data tree, and where in it each game's save
//! folder is, so a backup copy of each can be kept (`saves::backup`). The
//! trees are the ones romforge's config points each at (as in
//! `gamedata.rs` and `azahar.rs`): yours where it is, or RomForge's once
//! moved in, else the emulator's default inside romforge's config.
//!
//! - RPCS3: `dev_hdd0/home/00000001/savedata/<title ID…>/`.
//! - Cemu: `<mlc>/usr/save/<high>/<low>/` (the title ID's halves).
//! - Azahar, Lime3DS, Citra: `sdmc/Nintendo 3DS/<id0>/<id1>/title/<high>/
//!   <low>/data/`; none while the SD card is romforge's, in `Saves/`.
//! - Dolphin's Wii NAND: `Wii/title/<high>/<low>/data/`.
//!
//! Not here: xemu (its saves are inside the HDD image, deferred).

use std::path::{Path, PathBuf};

use super::existing::UserDirs;
use super::theirs::{cemu_mlc, citra_storage, moved_in, rpcs3_devices};
use crate::home::Home;

/// A data tree: its system, its root, and where its save folders are below
/// it (`*` any one folder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveTree {
    pub system: &'static str,
    pub root: PathBuf,
    pub pattern: &'static [&'static str],
}

/// `name`'s data tree, if it keeps saves outside `Saves/`.
pub fn save_tree(name: &str, home: &Home, user: Option<&UserDirs>) -> Option<SaveTree> {
    let dir = home.emulator_config(name);
    let (system, root, pattern): (_, PathBuf, &'static [&'static str]) = match name {
        "rpcs3" => {
            let hdd = if moved_in(&dir) {
                None
            } else {
                user.map(rpcs3_devices)
                    .and_then(|d| d.into_iter().find(|(dev, _)| *dev == "dev_hdd0"))
                    .map(|(_, p)| p)
            };
            let hdd = hdd.unwrap_or_else(|| dir.join("rpcs3/dev_hdd0"));
            ("PlayStation 3", hdd.join("home/00000001"), &["savedata", "*"])
        }
        "cemu" => {
            let mlc = user.filter(|_| !moved_in(&dir)).and_then(cemu_mlc);
            let mlc = mlc.unwrap_or_else(|| dir.join("data/Cemu/mlc01"));
            ("Wii U", mlc, &["usr", "save", "*", "*"])
        }
        "azahar" | "lime3ds" | "citra" => {
            let (_, reads) = super::azahar::folders(name)?;
            let storage = user
                .filter(|_| !moved_in(&dir))
                .and_then(|u| citra_storage(u, reads))?;
            (
                "Nintendo 3DS",
                storage.join("sdmc/Nintendo 3DS"),
                &["*", "*", "title", "*", "*", "data"],
            )
        }
        "dolphin" => ("Wii", dir.join("Wii"), &["title", "*", "*", "data"]),
        _ => return None,
    };
    Some(SaveTree {
        system,
        root,
        pattern,
    })
}

impl SaveTree {
    /// Each save folder in the tree, as its path below the root.
    pub fn folders(&self) -> Vec<PathBuf> {
        let mut at = vec![PathBuf::new()];
        for part in self.pattern {
            at = at.into_iter().flat_map(|rel| self.children(&rel, part)).collect();
        }
        at.sort();
        at
    }

    fn children(&self, rel: &Path, part: &str) -> Vec<PathBuf> {
        let here = self.root.join(rel);
        if part != "*" {
            return if here.join(part).is_dir() {
                vec![rel.join(part)]
            } else {
                Vec::new()
            };
        }
        std::fs::read_dir(&here)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| rel.join(e.file_name()))
            .collect()
    }
}
