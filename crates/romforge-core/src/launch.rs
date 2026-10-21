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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::{Error, systems};

pub mod controls;
mod emulators;
pub use emulators::*;
mod plan;
pub use plan::*;
mod owned;
pub use owned::{ConfigWrite, Prepare, existing, prepare};
mod plays;
pub use plays::*;
mod session;
pub use session::*;
mod start;
use start::*;
mod table;
pub use table::*;
mod tools;
pub use tools::*;

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
    /// Set in the emulator's environment.
    pub env: Vec<(String, String)>,
    /// romforge's config for the emulator, written by [`spawn`] first.
    pub prepare: Prepare,
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
/// environment (`Library::env` adds the RomForge folder and Settings);
/// tests point it elsewhere.
///
/// An emulator is found, in this order: at the path set for it in
/// Settings, on `PATH`, as a Flatpak, then in `emulator_dirs` as one of
/// its programs or an AppImage (`<name>*.AppImage`, any case).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    pub path: Vec<PathBuf>,
    pub core_dirs: Vec<PathBuf>,
    pub flatpak_dirs: Vec<PathBuf>,
    /// `~/Applications`, then `RomForge/Emulators`.
    pub emulator_dirs: Vec<PathBuf>,
    /// The program set in Emulator Settings, by settings name
    /// ([`KnownEmulator::settings_name`]).
    pub user_paths: BTreeMap<String, PathBuf>,
    /// The rest of Emulator Settings, by settings name; missing: the
    /// defaults (fullscreen, no extra arguments).
    pub options: BTreeMap<String, LaunchOptions>,
    /// The RomForge folder: emulators romforge writes a config for run
    /// from their copy in it. `None`: as they are set up.
    pub home: Option<crate::home::Home>,
    /// The user's home and XDG folders, where RPCS3, Cemu and xemu keep the
    /// game data romforge's config points at.
    pub user: Option<existing::UserDirs>,
}

/// How an emulator is started, as set in Emulator Settings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchOptions {
    /// Without its fullscreen flags.
    pub windowed: bool,
    /// Put after the fullscreen flags, before the game's arguments.
    pub args: Vec<String>,
    /// Keys set in romforge's copy of its config.
    pub config: Vec<(String, String)>,
    /// Keys romforge manages that aren't set: taken out of that copy.
    pub unset: Vec<String>,
    /// romforge's game pad hotkeys on (`launch::controls` writes what the
    /// emulator needs for them).
    pub hotkeys: bool,
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
            emulator_dirs: home.iter().map(|h| h.join("Applications")).collect(),
            user: existing::UserDirs::detect(),
            ..Env::default()
        }
    }

    /// Also look in a RomForge folder's `Emulators/` (and its `cores/`).
    pub fn with_emulators_dir(mut self, dir: &Path) -> Self {
        self.core_dirs.insert(0, dir.join("cores"));
        self.emulator_dirs.push(dir.to_path_buf());
        self
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
        self.known(&RETROARCH)
    }

    fn core(&self, name: &str) -> Option<PathBuf> {
        let file = format!("{name}_libretro.{}", std::env::consts::DLL_EXTENSION);
        self.core_dirs.iter().map(|d| d.join(&file)).find(|p| p.is_file())
    }

    fn known(&self, e: &KnownEmulator) -> Option<Runner> {
        let user = self
            .user_paths
            .get(e.settings_name())
            .filter(|p| is_executable(p));
        user.cloned()
            .or_else(|| e.programs.iter().find_map(|p| self.which(p)))
            .map(Runner::Program)
            .or_else(|| e.flatpak.and_then(|id| self.flatpak(id)).map(Runner::Flatpak))
            .or_else(|| self.in_emulator_dirs(e).map(Runner::Program))
    }

    /// One of `e`'s programs, else its newest-named AppImage, in the first
    /// emulator folder that has either.
    fn in_emulator_dirs(&self, e: &KnownEmulator) -> Option<PathBuf> {
        let prefix = e.appimage.unwrap_or(e.name).to_ascii_lowercase();
        self.emulator_dirs.iter().find_map(|d| {
            if let Some(p) = e.programs.iter().map(|p| d.join(p)).find(|p| is_executable(p)) {
                return Some(p);
            }
            let mut images: Vec<PathBuf> = std::fs::read_dir(d)
                .ok()?
                .filter_map(|f| f.ok().map(|f| f.path()))
                .filter(|p| {
                    let name = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase());
                    name.is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".appimage"))
                        && is_executable(p)
                })
                .collect();
            images.sort();
            images.pop()
        })
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
/// (often just home), so it gets `grants` (the game's folder, and the
/// RomForge folder for romforge's config) for this run: a game on another
/// drive opens too, and nothing is changed for good.
///
/// `env` is set for the emulator: in the process for a program, as
/// `--env=` for a Flatpak (which sets its own XDG folders otherwise). The
/// third part is what [`spawn`] sets.
fn command(
    runner: &Runner,
    args: Vec<String>,
    grants: &[PathBuf],
    env: &[(String, String)],
) -> (PathBuf, Vec<String>, Vec<(String, String)>) {
    match runner {
        Runner::Program(p) => (p.clone(), args, env.to_vec()),
        Runner::Flatpak(id) => {
            let mut a = vec!["run".to_owned()];
            for g in grants {
                a.push(format!("--filesystem={}", g.display()));
            }
            for (k, v) in env {
                a.push(format!("--env={k}={v}"));
            }
            a.push(id.clone());
            a.extend(args);
            (PathBuf::from("flatpak"), a, Vec::new())
        }
    }
}

/// What an emulator's arguments are filled with.
pub(crate) struct Vars<'a> {
    pub rom: &'a Path,
    /// An arcade game's DAT name, else the file's stem.
    pub set: &'a str,
    pub core: Option<&'a Path>,
    /// romforge's config folder for the emulator, if it writes one.
    pub config: Option<&'a Path>,
    /// The game's system (its key).
    pub system: &'a str,
}

/// An emulator's arguments for a game: `{rom}` the file, `{dir}` its
/// folder, `{set}` the set's name, `{core}` the RetroArch core.
fn fill(args: &[impl AsRef<str>], v: &Vars) -> Vec<String> {
    let dir = v.rom.parent().unwrap_or(Path::new(".")).to_string_lossy();
    let core = v.core.map(|c| c.to_string_lossy()).unwrap_or_default();
    let config = v.config.map(|c| c.to_string_lossy()).unwrap_or_default();
    args.iter()
        .map(|a| {
            a.as_ref()
                .replace("{rom}", &v.rom.to_string_lossy())
                .replace("{dir}", &dir)
                .replace("{set}", v.set)
                .replace("{core}", &core)
                .replace("{config}", &config)
                .replace("{system}", v.system)
        })
        .collect()
}

/// A known emulator's whole command line: its fullscreen flags (unless set
/// windowed), its config flags (with romforge's config), the extra
/// arguments set for it, then its arguments for the game.
fn known_args(e: &KnownEmulator, v: &Vars, o: &LaunchOptions) -> Vec<String> {
    let fullscreen = if o.windowed { &[][..] } else { e.fullscreen };
    let config = if v.config.is_some() {
        fill(e.config, v)
    } else {
        Vec::new()
    };
    fullscreen
        .iter()
        .map(|f| (*f).to_owned())
        .chain(config)
        .chain(o.args.iter().cloned())
        .chain(fill(e.args, v))
        .collect()
}

impl Env {
    /// How `e` would be started, if it is installed.
    pub fn find(&self, e: &KnownEmulator) -> Option<String> {
        self.known(e).map(|r| describe(&r))
    }

    /// Whether RetroArch's core `name` is installed.
    pub fn core_installed(&self, name: &str) -> bool {
        self.core(name).is_some()
    }

    /// Where RetroArch's core `name` is found.
    pub fn core_path(&self, name: &str) -> Option<PathBuf> {
        self.core(name)
    }

    fn options(&self, e: &KnownEmulator) -> LaunchOptions {
        self.options.get(e.settings_name()).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests;
