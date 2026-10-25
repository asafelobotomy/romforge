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
pub(crate) use formats::{merge, read};
mod azahar;
mod gamedata;
pub mod move_in;
pub(super) mod per_game;
mod standalone;
pub mod theirs;

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
    /// (theirs, ours): the user's own settings file, copied to start
    /// romforge's from when it has none yet ([`existing::seed`]).
    pub seeds: Vec<(PathBuf, PathBuf)>,
}

/// romforge's config for `e` running a game of `system`: its folder (the
/// `{config}` of the table's `config` flags) and what to write first,
/// Emulator Settings' keys (`options`) included. `None` for an emulator
/// romforge doesn't write a config for yet.
/// Cores whose user folder, system files included, is `<name>/` in
/// RetroArch's save folder (their "Save Data Location" default, LibRetro
/// Default; read in Azahar 2126.2's and libretro/citra's
/// `citra_libretro`): RetroBIOS's files for them (`Bios/RetroArch/<name>/`)
/// are linked in there.
const USER_DIR_IN_SAVES: &[(&str, &str)] = &[("azahar", "Azahar"), ("citra", "Citra")];

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
                // Game overrides (`owned/per_game.rs`) beside it.
                ("rgui_config_directory", s(dir.join("config"))),
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
            let (hot, remove, control) = super::controls::retroarch(options.hotkeys);
            keys.extend(hot);
            let links = USER_DIR_IN_SAVES
                .iter()
                .filter(|(core, _)| system.cores.contains(core))
                .map(|(_, d)| (bios.join(d), saves.join(d)))
                .collect();
            let prepare = Prepare {
                links,
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
            let prepare = with_settings(prepare, &dir, options);
            Some((dir, prepare))
        }
        name => {
            let bios = home.bios().join(name);
            standalone::standalone(name, &dir, &bios)
                .or_else(|| gamedata::gamedata(name, &dir, &bios, user))
                .or_else(|| azahar::citra_line(name, &dir, &bios, &home.saves().join(system.key), user))
                .or_else(|| file_only(e, &dir, user))
                .map(|mut p| {
                    super::controls::bind(name, options.hotkeys, &mut p.writes);
                    let p = with_settings(p, &dir, options);
                    (dir, p)
                })
        }
    }
}

/// The emulators romforge only points at its own settings file (the
/// table's `config` flags): DOSBox Staging, DOSBox-X, ScummVM and MAME,
/// that copy started from the user's own.
fn file_only(e: &KnownEmulator, dir: &Path, user: Option<&existing::UserDirs>) -> Option<Prepare> {
    let (file, format) = match e.settings_name() {
        "dosbox-staging" => ("dosbox-staging.conf", Format::Ini),
        "dosbox-x" => ("dosbox-x.conf", Format::Ini),
        "scummvm" => ("scummvm.ini", Format::Ini),
        "mame" => ("mame.ini", Format::Spaced),
        _ => return None,
    };
    Some(Prepare {
        dirs: vec![dir.to_path_buf()],
        writes: vec![ConfigWrite {
            path: dir.join(file),
            format,
            keys: Vec::new(),
            remove: Vec::new(),
        }],
        seeds: existing::seed(e, dir, user).into_iter().collect(),
        ..Prepare::default()
    })
}

/// `p` with Emulator Settings' keys (`options.config`) merged into its
/// writes of the same files; romforge's own keys there stay as they are.
fn with_settings(mut p: Prepare, dir: &Path, options: &LaunchOptions) -> Prepare {
    for set in &options.config {
        let path = dir.join(&set.path);
        let own = |k: &String| {
            p.writes
                .iter()
                .any(|w| w.path == path && w.keys.iter().any(|(o, _)| o == k))
        };
        let keys: Vec<_> = set.keys.iter().filter(|(k, _)| !own(k)).cloned().collect();
        let remove: Vec<_> = set.remove.iter().filter(|k| !own(k)).cloned().collect();
        match p.writes.iter_mut().find(|w| w.path == path) {
            Some(w) => {
                w.keys.extend(keys);
                w.remove.extend(remove);
            }
            None => p.writes.push(ConfigWrite {
                path,
                format: set.format,
                keys,
                remove,
            }),
        }
    }
    p
}

/// romforge's config for `e` as any of its games would have it (the first
/// system it runs), with `options`; `None` for one it doesn't write.
pub(crate) fn managed(
    e: &KnownEmulator,
    home: &Home,
    options: &LaunchOptions,
    user: Option<&existing::UserDirs>,
) -> Option<(PathBuf, Prepare)> {
    let system = crate::systems::SYSTEMS.iter().find(|s| {
        if e.settings_name() == "retroarch" {
            !s.cores.is_empty()
        } else {
            s.emulators
                .iter()
                .any(|n| super::known(n).is_some_and(|k| k.settings_name() == e.settings_name()))
        }
    })?;
    owned(e, home, system, options, user)
}

/// Make the folders, set the keys (each file written whole, with others'
/// keys, comments and order kept) and link BIOS files in.
pub fn prepare(p: &Prepare) -> Result<(), Error> {
    for d in &p.dirs {
        std::fs::create_dir_all(d)?;
    }
    for (theirs, ours) in &p.seeds {
        if !ours.exists() {
            crate::atomic::write(ours, &std::fs::read(theirs)?)?;
        }
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
/// `to`, where nothing is yet: a symbolic link to the file in `root` (the
/// RomForge folder keeps the one copy), else a hard link, else a copy. A
/// link of romforge's whose file has gone from `root` is made again.
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
        if let Ok(meta) = target.symlink_metadata() {
            let ours_broken = meta.file_type().is_symlink()
                && !target.exists()
                && std::fs::read_link(&target).is_ok_and(|l| l.starts_with(root));
            if !ours_broken {
                continue;
            }
            std::fs::remove_file(&target)?;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let source = std::path::absolute(&path)?;
        if std::os::unix::fs::symlink(&source, &target).is_err()
            && std::fs::hard_link(&path, &target).is_err()
        {
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
