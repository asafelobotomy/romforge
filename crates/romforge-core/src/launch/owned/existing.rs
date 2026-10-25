//! "Import my existing setup": an emulator romforge starts from its own
//! folder no longer sees what is in its usual one (memory cards, saves,
//! controller mappings, settings). This copies that across, once, when
//! the user asks.
//!
//! Where each keeps it was read from its source (`standalone.rs`); a
//! Flatpak keeps the same folders under `~/.var/app/<id>/`. Caches are
//! left behind. Files already in romforge's folder are never replaced,
//! except the emulator's own settings file, which becomes the base romforge
//! puts its keys back into at the next start (the one it replaces is kept
//! beside it for undo). Every file made is journaled.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::Error;
use crate::home::Home;
use crate::launch::KnownEmulator;

mod dirs;
pub use dirs::{UserDirs, Xdg};
mod saves;
mod undo;
pub use undo::undo;

/// One folder to copy: from the emulator's usual place to romforge's.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SetupSource {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// What an emulator's existing setup is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ExistingSetup {
    pub emulator: String,
    /// Folders found (empty: none on this computer).
    pub sources: Vec<SetupSource>,
    pub files: usize,
    pub bytes: u64,
    /// RetroArch's saves and states placed in romforge's per-platform
    /// folders ([`saves`]), and those whose platform can't be told.
    #[serde(default)]
    pub saves: usize,
    #[serde(default)]
    pub unplaced: usize,
    #[serde(skip)]
    save_pairs: Vec<(PathBuf, PathBuf)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SetupImported {
    pub copied: usize,
    pub bytes: u64,
    /// Of `copied`, saves and states.
    #[serde(default)]
    pub saves: usize,
    /// Already in romforge's folder, left as they were.
    pub kept: usize,
}

/// Folder names never copied (any case): caches and logs.
const SKIP: &[&str] = &["cache", "shadercache", "shaders", "logs", "dump", "graphicsmods"];

/// Folder names left where they are for one emulator: game data its
/// config points at instead of copies (RPCS3's devices, Cemu's MLC).
fn skip_for(name: &str) -> &'static [&'static str] {
    match name {
        "rpcs3" => &[
            "dev_hdd0",
            "dev_hdd1",
            "dev_flash",
            "dev_flash2",
            "dev_flash3",
            "dev_bdvd",
            "dev_usb000",
            "games",
        ],
        "cemu" => &["mlc01"],
        _ => &[],
    }
}

/// The settings files romforge writes keys into, relative to its folder:
/// imported files replace these, then romforge's keys go back in.
fn settings_files(name: &str) -> &'static [&'static str] {
    match name {
        "retroarch" => &["retroarch.cfg"],
        "duckstation" => &["duckstation/settings.ini"],
        "pcsx2" => &["PCSX2/inis/PCSX2.ini"],
        "mgba" => &["mgba/config.ini"],
        "mednafen" => &["mednafen.cfg"],
        "rpcs3" => &["rpcs3/vfs.yml", "rpcs3/GuiConfigs/CurrentSettings.ini"],
        "cemu" => &["Cemu/settings.xml"],
        "xemu" => &["xemu.toml"],
        "azahar" => &["azahar-emu/qt-config.ini"],
        "lime3ds" => &["lime3ds-emu/qt-config.ini"],
        "citra" => &["citra-emu/qt-config.ini"],
        "dosbox-staging" => &["dosbox-staging.conf"],
        "dosbox-x" => &["dosbox-x.conf"],
        "scummvm" => &["scummvm.ini"],
        "mame" => &["mame.ini"],
        _ => &[],
    }
}

/// `(usual folder, romforge's folder)` pairs for `e`, from the environment
/// (`$HOME`, `$XDG_*`), native first, then its Flatpak.
fn candidates(e: &KnownEmulator, dir: &Path, home_dir: &Path, xdg: &Xdg) -> Vec<SetupSource> {
    // `rel` under the XDG config (or data) folder, and under the Flatpak's.
    let both = |rel: &str, to: PathBuf, data: bool| -> Vec<SetupSource> {
        let (base, sub) = if data {
            (&xdg.data, "data")
        } else {
            (&xdg.config, "config")
        };
        let flatpak = e
            .flatpak
            .map(|id| home_dir.join(".var/app").join(id).join(sub).join(rel));
        std::iter::once(base.join(rel))
            .chain(flatpak)
            .map(|from| SetupSource { from, to: to.clone() })
            .collect()
    };
    let at = |from: PathBuf, to: PathBuf| vec![SetupSource { from, to }];
    match e.settings_name() {
        // Its settings file and overrides (romforge points
        // `rgui_config_directory` at its own): its other folders
        // (autoconfig, assets, remaps) are read where they are, through the
        // paths in it.
        "retroarch" => [
            both("retroarch/retroarch.cfg", dir.join("retroarch.cfg"), false),
            both("retroarch/config", dir.join("config"), false),
        ]
        .concat(),
        "duckstation" => [
            both("duckstation", dir.join("duckstation"), true),
            both("duckstation", dir.join("duckstation"), false),
        ]
        .concat(),
        "pcsx2" => both("PCSX2", dir.join("PCSX2"), false),
        "mgba" => both("mgba", dir.join("mgba"), false),
        "ppsspp" => both("ppsspp", dir.join("ppsspp"), false),
        "flycast" => [
            both("flycast", dir.join("flycast"), false),
            both("flycast", dir.join("data/flycast"), true),
        ]
        .concat(),
        "mednafen" => at(home_dir.join(".mednafen"), dir.to_path_buf()),
        "rpcs3" => both("rpcs3", dir.join("rpcs3"), false),
        "cemu" => [
            both("Cemu", dir.join("Cemu"), false),
            both("Cemu", dir.join("data/Cemu"), true),
        ]
        .concat(),
        "xemu" => both("xemu/xemu/xemu.toml", dir.join("xemu.toml"), true),
        // Citra's line: its config and system files, and those of the older
        // ones it reads; its NAND and SD card are used where they are
        // (`azahar.rs`).
        name @ ("azahar" | "lime3ds" | "citra") => {
            let (folder, reads) = super::azahar::folders(name).unwrap_or_default();
            reads
                .iter()
                .flat_map(|d| {
                    [
                        both(d, dir.join(folder), false),
                        both(
                            &format!("{d}/sysdata"),
                            dir.join("data").join(folder).join("sysdata"),
                            true,
                        ),
                    ]
                    .concat()
                })
                .collect()
        }
        "dolphin" => [
            at(home_dir.join(".dolphin-emu"), dir.to_path_buf()),
            both("dolphin-emu", dir.join("Config"), false),
            both("dolphin-emu", dir.to_path_buf(), true),
        ]
        .concat(),
        // Their one settings file (DOSBox Staging 0.83.0's
        // `get_platform_config_dir`, DOSBox-X's `dosbox-x.conf` beside its
        // versioned ones, ScummVM's XDG one, MAME's `~/.mame`).
        "dosbox-staging" => both(
            "dosbox/dosbox-staging.conf",
            dir.join("dosbox-staging.conf"),
            false,
        ),
        "dosbox-x" => both("dosbox-x/dosbox-x.conf", dir.join("dosbox-x.conf"), false),
        "scummvm" => both("scummvm/scummvm.ini", dir.join("scummvm.ini"), false),
        "mame" => at(home_dir.join(".mame/mame.ini"), dir.join("mame.ini")),
        _ => Vec::new(),
    }
}

/// The user's own settings file for `e` to start romforge's copy from,
/// when it has none yet (the emulators whose usual setup holds what they
/// need to find games: MAME's rompath, ScummVM's games): (theirs, ours).
pub(super) fn seed(e: &KnownEmulator, dir: &Path, user: Option<&UserDirs>) -> Option<(PathBuf, PathBuf)> {
    let user = user?;
    let ours: Vec<PathBuf> = settings_files(e.settings_name())
        .iter()
        .map(|f| dir.join(f))
        .collect();
    candidates(e, dir, &user.home, &user.xdg)
        .into_iter()
        .find(|c| c.from.is_file() && ours.contains(&c.to))
        .map(|c| (c.from, c.to))
}

/// Emulators that can have their setup imported: RetroArch and the
/// standalone ones romforge sets up.
pub fn importable(name: &str) -> bool {
    matches!(
        name,
        "retroarch"
            | "duckstation"
            | "pcsx2"
            | "mgba"
            | "ppsspp"
            | "flycast"
            | "mednafen"
            | "dolphin"
            | "rpcs3"
            | "cemu"
            | "xemu"
            | "azahar"
            | "lime3ds"
            | "citra"
            | "dosbox-staging"
            | "dosbox-x"
            | "scummvm"
            | "mame"
    )
}

/// What `e`'s existing setup holds (`conn`: the library, to tell
/// RetroArch's saves' platforms by their games).
pub fn existing(
    e: &KnownEmulator,
    home: &Home,
    home_dir: &Path,
    xdg: &Xdg,
    conn: Option<&Connection>,
) -> ExistingSetup {
    let dir = home.emulator_config(e.settings_name());
    let sources: Vec<SetupSource> = candidates(e, &dir, home_dir, xdg)
        .into_iter()
        .filter(|s| s.from.exists())
        .collect();
    let (mut files, mut bytes) = (0, 0);
    for s in &sources {
        each_file(&s.from, &s.to, skip_for(e.settings_name()), &mut |from, _| {
            files += 1;
            bytes += from.metadata().map(|m| m.len()).unwrap_or(0);
            Ok(())
        })
        .ok();
    }
    let placed = sources
        .iter()
        .find(|s| e.settings_name() == "retroarch" && s.from.ends_with("retroarch.cfg"))
        .map(|s| saves::retroarch(home, &s.from, home_dir, conn))
        .unwrap_or_default();
    ExistingSetup {
        emulator: e.settings_name().to_owned(),
        sources,
        files,
        bytes,
        saves: placed.pairs.len(),
        unplaced: placed.unplaced,
        save_pairs: placed.pairs,
    }
}

/// Every file under `from` (or `from` itself) with its place under `to`,
/// caches left out.
fn each_file(
    from: &Path,
    to: &Path,
    skip: &[&str],
    f: &mut dyn FnMut(&Path, &Path) -> Result<(), Error>,
) -> Result<(), Error> {
    if from.is_file() {
        return f(from, to);
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(from)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for p in entries {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if p.is_dir() {
            if !SKIP.contains(&name.as_str()) && !skip.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                each_file(&p, &to.join(p.file_name().unwrap_or_default()), skip, f)?;
            }
        } else if p.is_file() {
            f(&p, &to.join(p.file_name().unwrap_or_default()))?;
        }
    }
    Ok(())
}

/// Copy `e`'s existing setup into romforge's folder for it.
pub fn import(
    conn: &Connection,
    e: &KnownEmulator,
    home: &Home,
    home_dir: &Path,
    xdg: &Xdg,
) -> Result<SetupImported, Error> {
    let setup = existing(e, home, home_dir, xdg, Some(conn));
    if setup.sources.is_empty() {
        return Err(Error::Launch(format!(
            "no existing {} setup found on this computer",
            e.label
        )));
    }
    let dir = home.emulator_config(e.settings_name());
    let settings: Vec<PathBuf> = settings_files(e.settings_name())
        .iter()
        .map(|f| dir.join(f))
        .collect();
    conn.execute(
        concat!(
            "INSERT INTO setup_imports (emulator, created_at) VALUES (?1, ",
            crate::db::now!(),
            ")"
        ),
        [e.settings_name()],
    )?;
    let batch = conn.last_insert_rowid();
    let journal = |path: &Path, backup: Option<&Path>| -> Result<(), Error> {
        conn.execute(
            "INSERT OR IGNORE INTO setup_import_files (import_id, path, backup) VALUES (?1, ?2, ?3)",
            params![
                batch,
                path.to_string_lossy(),
                backup.map(|b| b.to_string_lossy().into_owned())
            ],
        )?;
        Ok(())
    };
    let mut done = SetupImported::default();
    for s in &setup.sources {
        each_file(&s.from, &s.to, skip_for(e.settings_name()), &mut |from, to| {
            let mut backup = None;
            if to.exists() {
                if !settings.iter().any(|p| p == to) {
                    done.kept += 1;
                    return Ok(());
                }
                let b = to.with_extension("romforge-before-import");
                if b.exists() {
                    done.kept += 1;
                    return Ok(());
                }
                std::fs::rename(to, &b)?;
                backup = Some(b);
            }
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(from, to)?;
            journal(to, backup.as_deref())?;
            done.copied += 1;
            done.bytes += from.metadata().map(|m| m.len()).unwrap_or(0);
            Ok(())
        })?;
    }
    // Saves and states, never over one there.
    for (from, to) in &setup.save_pairs {
        if to.exists() {
            done.kept += 1;
            continue;
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::organize::copy_checked(from, to)?;
        journal(to, None)?;
        done.copied += 1;
        done.saves += 1;
        done.bytes += from.metadata().map(|m| m.len()).unwrap_or(0);
    }
    Ok(done)
}

#[cfg(test)]
mod tests;
