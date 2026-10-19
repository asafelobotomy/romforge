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
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SetupImported {
    pub copied: usize,
    pub bytes: u64,
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
        // Its settings file only: its other folders (autoconfig, assets,
        // remaps) are read where they are, through the paths in it.
        "retroarch" => both("retroarch/retroarch.cfg", dir.join("retroarch.cfg"), false),
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
        "dolphin" => [
            at(home_dir.join(".dolphin-emu"), dir.to_path_buf()),
            both("dolphin-emu", dir.join("Config"), false),
            both("dolphin-emu", dir.to_path_buf(), true),
        ]
        .concat(),
        _ => Vec::new(),
    }
}

/// The user's home and XDG folders: where emulators keep their usual
/// setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDirs {
    pub home: PathBuf,
    pub xdg: Xdg,
}

impl UserDirs {
    /// From `$HOME` and the `$XDG_*` romforge runs with.
    pub fn detect() -> Option<UserDirs> {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        Some(UserDirs {
            xdg: Xdg::detect(&home),
            home,
        })
    }
}

/// The XDG base folders romforge itself runs with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Xdg {
    pub config: PathBuf,
    pub data: PathBuf,
}

impl Xdg {
    pub fn detect(home_dir: &Path) -> Xdg {
        let var = |v: &str, fallback: &str| {
            std::env::var_os(v)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home_dir.join(fallback))
        };
        Xdg {
            config: var("XDG_CONFIG_HOME", ".config"),
            data: var("XDG_DATA_HOME", ".local/share"),
        }
    }
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
    )
}

/// What `e`'s existing setup holds.
pub fn existing(e: &KnownEmulator, home: &Home, home_dir: &Path, xdg: &Xdg) -> ExistingSetup {
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
    ExistingSetup {
        emulator: e.settings_name().to_owned(),
        sources,
        files,
        bytes,
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
    let setup = existing(e, home, home_dir, xdg);
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
    Ok(done)
}

/// Take back the last setup import of `emulator`: the files it copied go,
/// settings files it replaced come back. Returns how many files went.
pub fn undo(conn: &Connection, emulator: &str) -> Result<usize, Error> {
    let batch: i64 = conn
        .query_row(
            "SELECT import_id FROM setup_imports WHERE emulator = ?1 AND undone_at IS NULL
             ORDER BY import_id DESC LIMIT 1",
            [emulator],
            |r| r.get(0),
        )
        .map_err(|_| Error::Launch(format!("no setup import of {emulator} to undo")))?;
    let mut stmt = conn.prepare("SELECT path, backup FROM setup_import_files WHERE import_id = ?1")?;
    let rows: Vec<(String, Option<String>)> = stmt
        .query_map([batch], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut n = 0;
    for (path, backup) in rows {
        match std::fs::remove_file(&path) {
            Ok(()) => n += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        if let Some(b) = backup {
            std::fs::rename(&b, &path)?;
        }
    }
    conn.execute(
        concat!(
            "UPDATE setup_imports SET undone_at = ",
            crate::db::now!(),
            " WHERE import_id = ?1"
        ),
        [batch],
    )?;
    Ok(n)
}

#[cfg(test)]
mod tests;
