//! Milestone 18, 1.3: an emulator runs from romforge's own copy of its
//! config, in `RomForge/Emulators/config/<emulator>/`, never from the one
//! it uses when started outside romforge. romforge sets the keys below on
//! every start (folders into RomForge, no saving its config on exit) and
//! keeps every other key as it is, so a setting changed elsewhere in that
//! copy survives.
//!
//! RetroArch first: `retroarch.cfg` for what is the same everywhere, and
//! `systems/<system>.cfg`, given with `--appendconfig`, for the folders of
//! one system (RetroArch has one save folder, so each system gets its own
//! file). The keys were checked in RetroArch 1.22.2's own config.

use std::path::{Path, PathBuf};

use super::KnownEmulator;
use crate::home::Home;
use crate::{Error, systems::System};

/// A config file and the keys romforge sets in it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ConfigWrite {
    pub path: PathBuf,
    pub keys: Vec<(String, String)>,
}

/// What has to exist before an emulator starts from romforge's config.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Prepare {
    pub dirs: Vec<PathBuf>,
    pub writes: Vec<ConfigWrite>,
}

/// romforge's config for `e` running a game of `system`: its folder (the
/// `{config}` of the table's `config` flags) and what to write first.
/// `None` for an emulator romforge doesn't write a config for yet.
pub(super) fn owned(e: &KnownEmulator, home: &Home, system: &System) -> Option<(PathBuf, Prepare)> {
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
            let write = |path: PathBuf, keys: &[(&str, String)]| ConfigWrite {
                path,
                keys: keys.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect(),
            };
            let prepare = Prepare {
                dirs: vec![bios, saves, states, shots],
                writes: vec![
                    write(dir.join("retroarch.cfg"), &base),
                    write(
                        dir.join("systems").join(format!("{}.cfg", system.key)),
                        &per_system,
                    ),
                ],
            };
            Some((dir, prepare))
        }
        _ => None,
    }
}

/// Make the folders and set the keys, each file written whole (others'
/// keys, comments and order kept).
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
        let new = merge_cfg(&old, &w.keys)?;
        if new != old {
            crate::atomic::write(&w.path, new.as_bytes())?;
        }
    }
    Ok(())
}

/// `text` (RetroArch's `key = "value"` lines) with `keys` set: a line for
/// a key is replaced where it is, a key with no line is added at the end.
fn merge_cfg(text: &str, keys: &[(String, String)]) -> Result<String, Error> {
    let line = |k: &str, v: &str| -> Result<String, Error> {
        if v.contains('"') || v.contains('\n') {
            return Err(Error::Launch(format!(
                "{k}: can't write {v:?} into a RetroArch config"
            )));
        }
        Ok(format!("{k} = \"{v}\""))
    };
    let key_of = |l: &str| l.split_once('=').map(|(k, _)| k.trim().to_owned());
    let mut done = vec![false; keys.len()];
    let mut out = String::new();
    for l in text.lines() {
        match key_of(l).and_then(|k| keys.iter().position(|(key, _)| *key == k)) {
            Some(i) if !done[i] => {
                done[i] = true;
                out.push_str(&line(&keys[i].0, &keys[i].1)?);
            }
            Some(_) => continue,
            None => out.push_str(l),
        }
        out.push('\n');
    }
    for (i, (k, v)) in keys.iter().enumerate() {
        if !done[i] {
            out.push_str(&line(k, v)?);
            out.push('\n');
        }
    }
    Ok(out)
}

/// The folders a Flatpak emulator must be able to read: the game's and,
/// with romforge's config, the RomForge folder.
pub(super) fn grants(game: Option<&Path>, home: Option<&Home>) -> Vec<PathBuf> {
    game.map(Path::to_path_buf)
        .into_iter()
        .chain(home.map(|h| h.root().to_path_buf()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_set_and_the_rest_kept() {
        let text =
            "# mine\nvideo_driver = \"vulkan\"\nsystem_directory = \"/old\"\nsystem_directory = \"/dup\"\n";
        let keys = vec![
            ("system_directory".to_owned(), "/rf/Bios/RetroArch".to_owned()),
            ("config_save_on_exit".to_owned(), "false".to_owned()),
        ];
        assert_eq!(
            merge_cfg(text, &keys).unwrap(),
            "# mine\nvideo_driver = \"vulkan\"\nsystem_directory = \"/rf/Bios/RetroArch\"\nconfig_save_on_exit = \"false\"\n"
        );
        assert_eq!(merge_cfg("", &keys).unwrap().lines().count(), 2);
        let quoted = vec![("x".to_owned(), "a\"b".to_owned())];
        assert!(merge_cfg("", &quoted).is_err());
    }
}
