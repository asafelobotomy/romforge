//! Milestone 5: start a game in RetroArch or a standalone emulator.
//!
//! Which emulator: one you pass by name, else the first that is installed
//! of the system's RetroArch `cores` and then its standalone `emulators`
//! (both columns of `systems.rs`), with your own `emulator add` entries
//! first. RetroArch is found on `PATH` or as the Flatpak
//! `org.libretro.RetroArch`, its cores in the usual core folders.
//!
//! What to start: a game's matched file. A ROM inside a zip/7z is handed to
//! RetroArch as the archive (RetroArch opens archives itself); a standalone
//! emulator gets it only if it is marked as reading archives, since romforge
//! never extracts. For a loose disc set the sheet (`.cue`, `.gdi`, `.m3u`)
//! is started, not a track.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::{Error, systems};

mod emulators;
pub use emulators::*;
mod plan;
pub use plan::*;
mod start;
use start::*;
mod tools;
pub use tools::*;

/// A standalone emulator romforge knows how to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownEmulator {
    pub name: &'static str,
    /// Executable names to look for on `PATH`.
    pub programs: &'static [&'static str],
    pub flatpak: Option<&'static str>,
    /// Arguments; `{rom}` is the file.
    pub args: &'static [&'static str],
    /// Opens a ROM inside a zip/7z itself.
    pub archives: bool,
}

const fn emu(
    name: &'static str,
    programs: &'static [&'static str],
    flatpak: &'static str,
    args: &'static [&'static str],
    archives: bool,
) -> KnownEmulator {
    KnownEmulator {
        name,
        programs,
        flatpak: Some(flatpak),
        args,
        archives,
    }
}

pub const KNOWN_EMULATORS: &[KnownEmulator] = &[
    emu(
        "duckstation",
        &["duckstation-qt", "duckstation"],
        "org.duckstation.DuckStation",
        &["{rom}"],
        false,
    ),
    emu(
        "pcsx2",
        &["pcsx2-qt", "pcsx2"],
        "net.pcsx2.PCSX2",
        &["{rom}"],
        false,
    ),
    emu(
        "dolphin",
        &["dolphin-emu"],
        "org.DolphinEmu.dolphin-emu",
        &["-b", "-e", "{rom}"],
        false,
    ),
    emu(
        "ppsspp",
        &["PPSSPPSDL", "ppsspp"],
        "org.ppsspp.PPSSPP",
        &["{rom}"],
        false,
    ),
    emu("flycast", &["flycast"], "org.flycast.Flycast", &["{rom}"], false),
    emu("mgba", &["mgba-qt", "mgba"], "io.mgba.mGBA", &["{rom}"], true),
    emu("melonds", &["melonDS"], "net.kuribo64.melonDS", &["{rom}"], true),
    emu("xemu", &["xemu"], "app.xemu.xemu", &["-dvd_path", "{rom}"], false),
    emu(
        "rpcs3",
        &["rpcs3"],
        "net.rpcs3.RPCS3",
        &["--no-gui", "{rom}"],
        false,
    ),
    emu("mednafen", &["mednafen"], "com.github.mednafen", &["{rom}"], true),
    emu(
        "cemu",
        &["cemu", "Cemu"],
        "info.cemu.Cemu",
        &["-g", "{rom}"],
        false,
    ),
    // A folder is mounted as C:, a program run from its folder; images
    // and archives need the core.
    emu(
        "dosbox-staging",
        &["dosbox-staging", "dosbox"],
        "io.github.dosbox-staging",
        &["{rom}"],
        false,
    ),
    emu(
        "dosbox-x",
        &["dosbox-x"],
        "com.dosbox_x.DOSBox-X",
        &["{rom}"],
        false,
    ),
    // ScummVM finds the game in the folder itself.
    emu(
        "scummvm",
        &["scummvm"],
        "org.scummvm.ScummVM",
        &["--path={rom}", "--auto-detect"],
        false,
    ),
    // Arcade sets by name, from the folder holding them (and their parents
    // and BIOS sets).
    emu(
        "mame",
        &["mame"],
        "org.mamedev.MAME",
        &["-rompath", "{dir}", "{set}"],
        true,
    ),
    // MAME needs the machine's own ROMs in its rompath.
    emu(
        "mame-fm7",
        &["mame"],
        "org.mamedev.MAME",
        &["fm7", "-flop1", "{rom}"],
        false,
    ),
    emu(
        "mame-fmtowns",
        &["mame"],
        "org.mamedev.MAME",
        &["fmtowns", "-cdrom", "{rom}"],
        false,
    ),
];

pub const RETROARCH_FLATPAK: &str = "org.libretro.RetroArch";

/// How a program is started.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum Runner {
    /// An executable path.
    Program(PathBuf),
    /// `flatpak run <id>`.
    Flatpak(String),
}

/// What [`plan`] decided: run `program` with `args`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LaunchSpec {
    pub game: String,
    pub system: String,
    pub emulator: String,
    pub rom: PathBuf,
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// An emulator a user added (`emulator add`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UserEmulator {
    pub name: String,
    pub system: String,
    pub program: String,
    pub args: Vec<String>,
    pub archives: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EmulatorStatus {
    pub name: String,
    pub systems: Vec<String>,
    /// How it would be started, if installed.
    pub found: Option<String>,
}

/// Where to look for installed emulators. [`Env::detect`] reads the real
/// environment; tests point it elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Env {
    pub path: Vec<PathBuf>,
    pub core_dirs: Vec<PathBuf>,
    pub flatpak_dirs: Vec<PathBuf>,
}

impl Env {
    pub fn detect() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let path = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        let mut core_dirs = Vec::new();
        if let Some(h) = &home {
            core_dirs.push(h.join(".config/retroarch/cores"));
            core_dirs.push(h.join(format!(".var/app/{RETROARCH_FLATPAK}/config/retroarch/cores")));
        }
        for d in [
            "/usr/lib/libretro",
            "/usr/lib64/libretro",
            "/usr/lib/x86_64-linux-gnu/libretro",
            "/usr/local/lib/libretro",
        ] {
            core_dirs.push(PathBuf::from(d));
        }
        let mut flatpak_dirs = vec![PathBuf::from("/var/lib/flatpak/app")];
        if let Some(h) = &home {
            flatpak_dirs.push(h.join(".local/share/flatpak/app"));
        }
        Env {
            path,
            core_dirs,
            flatpak_dirs,
        }
    }

    fn which(&self, program: &str) -> Option<PathBuf> {
        let p = Path::new(program);
        if p.is_absolute() {
            return p.is_file().then(|| p.to_path_buf());
        }
        self.path
            .iter()
            .map(|d| d.join(program))
            .find(|c| is_executable(c))
    }

    fn flatpak(&self, id: &str) -> Option<String> {
        self.flatpak_dirs
            .iter()
            .any(|d| d.join(id).is_dir())
            .then(|| id.to_owned())
    }

    fn retroarch(&self) -> Option<Runner> {
        self.which("retroarch")
            .map(Runner::Program)
            .or_else(|| self.flatpak(RETROARCH_FLATPAK).map(Runner::Flatpak))
    }

    fn core(&self, name: &str) -> Option<PathBuf> {
        let file = format!("{name}_libretro.{}", std::env::consts::DLL_EXTENSION);
        self.core_dirs.iter().map(|d| d.join(&file)).find(|p| p.is_file())
    }

    fn known(&self, e: &KnownEmulator) -> Option<Runner> {
        e.programs
            .iter()
            .find_map(|p| self.which(p))
            .map(Runner::Program)
            .or_else(|| e.flatpak.and_then(|id| self.flatpak(id)).map(Runner::Flatpak))
    }
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// The command line for `runner`. A Flatpak sees only what it was given
/// (often just home), so it gets `grant` (the game's folder) for this run:
/// a game on another drive opens too, and nothing is changed for good.
fn command(runner: &Runner, args: Vec<String>, grant: Option<&Path>) -> (PathBuf, Vec<String>) {
    match runner {
        Runner::Program(p) => (p.clone(), args),
        Runner::Flatpak(id) => {
            let mut a = vec!["run".to_owned()];
            if let Some(g) = grant {
                a.push(format!("--filesystem={}", g.display()));
            }
            a.push(id.clone());
            a.extend(args);
            (PathBuf::from("flatpak"), a)
        }
    }
}

/// An emulator's arguments for a game: `{rom}` the file, `{dir}` its
/// folder, `{set}` the set's name (an arcade game's DAT name, else the
/// file's stem).
fn fill(args: &[impl AsRef<str>], rom: &Path, set: &str) -> Vec<String> {
    let dir = rom.parent().unwrap_or(Path::new(".")).to_string_lossy();
    args.iter()
        .map(|a| {
            a.as_ref()
                .replace("{rom}", &rom.to_string_lossy())
                .replace("{dir}", &dir)
                .replace("{set}", set)
        })
        .collect()
}

#[cfg(test)]
mod tests;
