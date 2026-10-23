//! Milestone 18, 1.3: an emulator runs from romforge's own copy of its
//! config, in `RomForge/Emulators/config/<emulator>/`, never from the one
//! it uses when started outside romforge. romforge sets the keys below on
//! every start (folders into RomForge, no saving its config on exit) and
//! keeps every other key as it is, so a setting changed elsewhere in that
//! copy survives.
//!
//! RetroArch: `retroarch.cfg` for what is the same everywhere, and
//! `systems/<system>.cfg`, given with `--appendconfig`, for the folders of
//! one system (RetroArch has one save folder, so each system gets its own
//! file). The keys were checked in RetroArch 1.22.2's own config. The
//! standalone emulators are in `owned/standalone.rs`.

use std::path::{Path, PathBuf};

use super::{KnownEmulator, LaunchOptions};
use crate::home::Home;
use crate::{Error, systems::System};

pub mod existing;
mod formats;
pub use formats::Format;
mod gamedata;
mod standalone;

/// A config file, the keys romforge sets in it and the ones it takes out.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ConfigWrite {
    pub path: PathBuf,
    pub format: Format,
    pub keys: Vec<(String, String)>,
    pub remove: Vec<String>,
}

/// What has to exist before an emulator starts from romforge's config,
/// and the environment it starts with.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Prepare {
    pub dirs: Vec<PathBuf>,
    pub writes: Vec<ConfigWrite>,
    pub env: Vec<(String, String)>,
    /// (from, to): every file under `from` is linked to the same place
    /// under `to`, where nothing is yet (an emulator with no key for its
    /// BIOS files).
    pub links: Vec<(PathBuf, PathBuf)>,
    /// How romforge reaches it while it runs, for the hotkeys.
    pub control: Option<super::controls::Channel>,
}

/// romforge's config for `e` running a game of `system`: its folder (the
/// `{config}` of the table's `config` flags) and what to write first,
/// Emulator Settings' keys (`options`) included. `None` for an emulator
/// romforge doesn't write a config for yet.
pub(super) fn owned(
    e: &KnownEmulator,
    home: &Home,
    system: &System,
    options: &LaunchOptions,
    user: Option<&existing::UserDirs>,
) -> Option<(PathBuf, Prepare)> {
    let dir = home.emulator_config(e.settings_name());
    let s = |p: PathBuf| p.to_string_lossy().into_owned();
    match e.settings_name() {
        "retroarch" => {
            let bios = home.bios().join("RetroArch");
            let (saves, states, shots) = (
                home.saves().join(system.key),
                home.states().join(system.key),
                home.screenshots().join(system.key),
            );
            let base = [
                ("system_directory", s(bios.clone())),
                ("config_save_on_exit", "false".into()),
                ("pause_nonactive", "false".into()),
                ("savefiles_in_content_dir", "false".into()),
                ("savestates_in_content_dir", "false".into()),
                ("systemfiles_in_content_dir", "false".into()),
                ("screenshots_in_content_dir", "false".into()),
                ("sort_savefiles_enable", "false".into()),
                ("sort_savestates_enable", "false".into()),
                ("sort_savefiles_by_content_enable", "false".into()),
                ("sort_savestates_by_content_enable", "false".into()),
                ("sort_screenshots_by_content_enable", "false".into()),
            ];
            let per_system = [
                ("savefile_directory", s(saves.clone())),
                ("savestate_directory", s(states.clone())),
                ("screenshot_directory", s(shots.clone())),
            ];
            let pairs = |keys: &[(&str, String)]| -> Vec<(String, String)> {
                keys.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
            };
            let mut keys = pairs(&base);
            keys.extend(options.config.iter().cloned());
            let (hot, mut remove, control) = super::controls::retroarch(options.hotkeys);
            keys.extend(hot);
            remove.extend(options.unset.iter().cloned());
            let prepare = Prepare {
                dirs: vec![bios, saves, states, shots],
                writes: vec![
                    ConfigWrite {
                        path: dir.join("retroarch.cfg"),
                        format: Format::RetroArch,
                        keys,
                        remove,
                    },
                    ConfigWrite {
                        path: dir.join("systems").join(format!("{}.cfg", system.key)),
                        format: Format::RetroArch,
                        keys: pairs(&per_system),
                        remove: Vec::new(),
                    },
                ],
                control,
                ..Prepare::default()
            };
            Some((dir, prepare))
        }
        name => {
            let bios = home.bios().join(name);
            standalone::standalone(name, &dir, &bios)
                .or_else(|| gamedata::gamedata(name, &dir, &bios, user))
                .map(|mut p| {
                    super::controls::bind(name, options.hotkeys, &mut p.writes);
                    (dir, p)
                })
        }
    }
}

/// Make the folders, set the keys (each file written whole, with others'
/// keys, comments and order kept) and link BIOS files in.
pub fn prepare(p: &Prepare) -> Result<(), Error> {
    for d in &p.dirs {
        std::fs::create_dir_all(d)?;
    }
    for w in &p.writes {
        let old = match std::fs::read_to_string(&w.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.into()),
        };
        let new = formats::merge(w.format, &old, &w.keys, &w.remove)?;
        if new != old || !w.path.exists() {
            crate::atomic::write(&w.path, new.as_bytes())?;
        }
    }
    for (from, to) in &p.links {
        link_tree(from, from, to)?;
    }
    Ok(())
}

/// Link each file under `dir` (inside `root`) to the same place under
/// `to`, where nothing is yet: a hard link, else a copy.
fn link_tree(root: &Path, dir: &Path, to: &Path) -> Result<(), Error> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for e in entries.filter_map(Result::ok) {
        let path = e.path();
        if path.is_dir() {
            link_tree(root, &path, to)?;
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let target = to.join(rel);
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if std::fs::hard_link(&path, &target).is_err() {
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

/// The folders a Flatpak emulator must be able to read: the game's and,
/// when romforge sets it up from there, the RomForge folder.
pub(super) fn grants(game: Option<&Path>, home: Option<&Home>) -> Vec<PathBuf> {
    game.map(Path::to_path_buf)
        .into_iter()
        .chain(home.map(|h| h.root().to_path_buf()))
        .collect()
}
