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

/// The settings files romforge writes keys into, relative to its folder:
/// imported files replace these, then romforge's keys go back in.
fn settings_files(name: &str) -> &'static [&'static str] {
    match name {
        "retroarch" => &["retroarch.cfg"],
        "duckstation" => &["duckstation/settings.ini"],
        "pcsx2" => &["PCSX2/inis/PCSX2.ini"],
        "mgba" => &["mgba/config.ini"],
        "mednafen" => &["mednafen.cfg"],
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
        "dolphin" => [
            at(home_dir.join(".dolphin-emu"), dir.to_path_buf()),
            both("dolphin-emu", dir.join("Config"), false),
            both("dolphin-emu", dir.to_path_buf(), true),
        ]
        .concat(),
        _ => Vec::new(),
    }
}

/// The XDG base folders romforge itself runs with.
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
        "retroarch" | "duckstation" | "pcsx2" | "mgba" | "ppsspp" | "flycast" | "mednafen" | "dolphin"
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
        each_file(&s.from, &s.to, &mut |from, _| {
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
            if !SKIP.contains(&name.as_str()) {
                each_file(&p, &to.join(p.file_name().unwrap_or_default()), f)?;
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
        each_file(&s.from, &s.to, &mut |from, to| {
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
mod tests {
    use super::super::{prepare, standalone::standalone};
    use super::*;

    #[test]
    fn an_existing_setup_comes_across_and_goes_back() {
        let t = tempfile::tempdir().unwrap();
        let user = t.path().join("user");
        let xdg = Xdg {
            config: user.join(".config"),
            data: user.join(".local/share"),
        };
        let home = Home::create(&t.path().join("parent")).unwrap();
        let pcsx2 = crate::launch::known("pcsx2").unwrap();
        assert!(existing(pcsx2, &home, &user, &xdg).sources.is_empty());

        // The user's PCSX2: settings with a controller mapping, a memory
        // card, a cache; and a Flatpak copy too.
        let theirs = xdg.config.join("PCSX2");
        std::fs::create_dir_all(theirs.join("inis")).unwrap();
        std::fs::create_dir_all(theirs.join("memcards")).unwrap();
        std::fs::create_dir_all(theirs.join("cache")).unwrap();
        std::fs::write(theirs.join("inis/PCSX2.ini"), "[Pad1]\nType = DualShock2\n").unwrap();
        std::fs::write(theirs.join("memcards/Mcd001.ps2"), b"card").unwrap();
        std::fs::write(theirs.join("cache/gamelist.cache"), b"big").unwrap();
        let flat = user.join(".var/app/net.pcsx2.PCSX2/config/PCSX2/memcards");
        std::fs::create_dir_all(&flat).unwrap();
        std::fs::write(flat.join("Mcd001.ps2"), b"flatpak card").unwrap();

        // romforge already started it once (its own settings file), and a
        // memory card of romforge's is there.
        let dir = home.emulator_config("pcsx2");
        let bios = home.bios().join("pcsx2");
        prepare(&standalone("pcsx2", &dir, &bios).unwrap()).unwrap();

        let found = existing(pcsx2, &home, &user, &xdg);
        assert_eq!(found.sources.len(), 2);
        assert_eq!(found.files, 3, "the cache is left out");

        let conn = crate::db::open_in_memory().unwrap();
        let done = import(&conn, pcsx2, &home, &user, &xdg).unwrap();
        assert_eq!(
            (done.copied, done.kept),
            (2, 1),
            "the Flatpak's card doesn't replace the native one"
        );
        assert_eq!(
            std::fs::read(dir.join("PCSX2/memcards/Mcd001.ps2")).unwrap(),
            b"card"
        );
        assert!(!dir.join("PCSX2/cache").exists());
        let ini = dir.join("PCSX2/inis/PCSX2.ini");
        assert_eq!(
            std::fs::read_to_string(&ini).unwrap(),
            "[Pad1]\nType = DualShock2\n"
        );
        // The next start puts romforge's keys back into the user's file.
        prepare(&standalone("pcsx2", &dir, &bios).unwrap()).unwrap();
        let text = std::fs::read_to_string(&ini).unwrap();
        assert!(
            text.starts_with("[Pad1]\nType = DualShock2\n") && text.contains("SettingsVersion = 1"),
            "{text}"
        );

        // Undo: the copies go, romforge's own settings file comes back.
        assert_eq!(undo(&conn, "pcsx2").unwrap(), 2);
        assert!(!dir.join("PCSX2/memcards/Mcd001.ps2").exists());
        assert!(
            std::fs::read_to_string(&ini)
                .unwrap()
                .contains("[UI]\nSettingsVersion = 1")
        );
        assert!(undo(&conn, "pcsx2").is_err());
        assert!(
            theirs.join("memcards/Mcd001.ps2").exists(),
            "the user's own files are untouched"
        );
    }
}
