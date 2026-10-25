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
pub(crate) use owned::save_trees::{SaveTree, save_tree};
pub use owned::{ConfigWrite, Format, Prepare, existing, move_in, prepare};
pub(crate) use owned::{managed, merge, read};
mod plays;
pub use plays::*;
mod session;
pub use session::*;
mod start;
use start::*;
mod running;
mod table;
pub use running::running;
pub use table::*;
mod tools;
pub use tools::*;
mod command;
use command::command;
mod flatpak;

/// How a program is started.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum Runner {
    /// An executable path.
    Program(PathBuf),
    /// `flatpak run <id>`; `command`: the app's own command, from its
    /// `metadata` (for setting its environment, [`command`]).
    Flatpak { id: String, command: Option<String> },
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
    /// romforge's config for the emulator, written first by
    /// [`Session::start`] (or [`spawn`]).
    pub prepare: Prepare,
    /// The choices passed over before this one, and why (a core without
    /// its BIOS file, one not installed).
    #[serde(default)]
    pub skipped: Vec<String>,
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
/// Settings, on `PATH`, its official AppImage in `emulator_dirs` where the
/// table names one (RPCS3's), as a Flatpak, then in `emulator_dirs` as one
/// of its programs or an AppImage (`<name>*.AppImage`, any case).
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
    /// The folder of downloaded lists holding RetroBIOS's, for the BIOS
    /// check before a game starts (`firmware::gate`); `None`: no check.
    pub firmware_list: Option<PathBuf>,
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
    /// Emulator Settings' keys, by file (each `path` from romforge's
    /// config folder for it): set ones written, unset ones (`remove`) taken
    /// out, so the emulator's own default applies.
    pub config: Vec<ConfigWrite>,
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
        // Its official AppImage, where it has one, before its official
        // Flatpak (one from elsewhere is never used, `launch/flatpak.rs`).
        let official = || e.official_appimage.and_then(|_| self.in_emulator_dirs(e));
        user.cloned()
            .or_else(|| e.programs.iter().find_map(|p| self.which(p)))
            .or_else(official)
            .map(Runner::Program)
            .or_else(|| self.flatpak(e))
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

/// What an emulator's arguments are filled with.
pub(crate) struct Vars<'a> {
    pub rom: &'a Path,
    /// An arcade game's DAT name, else the file's stem.
    pub set: &'a str,
    pub core: Option<&'a Path>,
    /// romforge's config folder for the emulator, if it writes one.
    pub config: Option<&'a Path>,
    /// Its folder in the RomForge folder's `Bios/`: for a known emulator
    /// only when it holds a file (its `bios` flags come with it).
    pub bios: Option<&'a Path>,
    /// The game's system (its key).
    pub system: &'a str,
}

/// An emulator's arguments for a game: `{rom}` the file, `{dir}` its
/// folder, `{set}` the set's name, `{core}` the RetroArch core, `{config}`
/// romforge's config folder, `{bios}` its BIOS folder, `{system}` the
/// system. One pass: a value that
/// itself holds a placeholder's text (a ROM named `{set}.zip`) stays as it is.
fn fill(args: &[impl AsRef<str>], v: &Vars) -> Vec<String> {
    let rom = v.rom.to_string_lossy();
    let dir = v.rom.parent().unwrap_or(Path::new(".")).to_string_lossy();
    let core = v.core.map(|c| c.to_string_lossy()).unwrap_or_default();
    let config = v.config.map(|c| c.to_string_lossy()).unwrap_or_default();
    let bios = v.bios.map(|c| c.to_string_lossy()).unwrap_or_default();
    let vars: [(&str, &str); 7] = [
        ("{rom}", &rom),
        ("{dir}", &dir),
        ("{set}", v.set),
        ("{core}", &core),
        ("{config}", &config),
        ("{bios}", &bios),
        ("{system}", v.system),
    ];
    args.iter()
        .map(|a| {
            let mut rest = a.as_ref();
            let mut out = String::with_capacity(rest.len());
            while let Some(i) = rest.find('{') {
                out.push_str(&rest[..i]);
                rest = &rest[i..];
                match vars.iter().find(|(k, _)| rest.starts_with(k)) {
                    Some((k, value)) => {
                        out.push_str(value);
                        rest = &rest[k.len()..];
                    }
                    None => {
                        out.push('{');
                        rest = &rest[1..];
                    }
                }
            }
            out.push_str(rest);
            out
        })
        .collect()
}

/// A known emulator's whole command line: its fullscreen flags (unless set
/// windowed), its config flags (with romforge's config), its BIOS folder's
/// flags (with a RomForge folder), the extra arguments set for it, then its
/// arguments for the game.
fn known_args(e: &KnownEmulator, v: &Vars, o: &LaunchOptions) -> Vec<String> {
    let fullscreen = if o.windowed { &[][..] } else { e.fullscreen };
    let config = if v.config.is_some() {
        fill(e.config, v)
    } else {
        Vec::new()
    };
    let bios = if v.bios.is_some() {
        fill(e.bios, v)
    } else {
        Vec::new()
    };
    fullscreen
        .iter()
        .map(|f| (*f).to_owned())
        .chain(config)
        .chain(bios)
        .chain(o.args.iter().cloned())
        .chain(fill(e.args, v))
        .collect()
}

impl Env {
    /// How `e` would be started, if it is installed.
    pub fn find(&self, e: &KnownEmulator) -> Option<String> {
        self.known(e).map(|r| describe(&r))
    }

    /// Where RetroArch's core `name` is found.
    pub fn core_path(&self, name: &str) -> Option<PathBuf> {
        self.core(name)
    }

    pub(crate) fn options(&self, e: &KnownEmulator) -> LaunchOptions {
        self.options.get(e.settings_name()).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests;
