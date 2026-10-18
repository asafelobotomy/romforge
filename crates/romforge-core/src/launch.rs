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

fn fill(args: &[impl AsRef<str>], rom: &Path) -> Vec<String> {
    args.iter()
        .map(|a| a.as_ref().replace("{rom}", &rom.to_string_lossy()))
        .collect()
}

pub fn user_emulators(conn: &Connection) -> Result<Vec<UserEmulator>, Error> {
    let mut stmt = conn.prepare(
        "SELECT e.name, s.key, e.program, e.args, e.archives FROM emulators e
         JOIN systems s ON s.system_id = e.system_id ORDER BY e.emulator_id",
    )?;
    let rows = stmt.query_map([], |r| {
        let args: String = r.get(3)?;
        Ok(UserEmulator {
            name: r.get(0)?,
            system: r.get(1)?,
            program: r.get(2)?,
            args: serde_json::from_str(&args).unwrap_or_default(),
            archives: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn add_emulator(conn: &Connection, e: &UserEmulator) -> Result<(), Error> {
    let system = systems::by_key(&e.system).ok_or_else(|| Error::UnknownSystem(e.system.clone()))?;
    if e.args.iter().all(|a| !a.contains("{rom}")) {
        return Err(Error::Launch("the arguments need a {rom} placeholder".into()));
    }
    conn.execute(
        "INSERT INTO emulators (name, system_id, program, args, archives) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(name, system_id) DO UPDATE SET program = excluded.program, args = excluded.args,
           archives = excluded.archives",
        params![
            e.name,
            system.id,
            e.program,
            serde_json::to_string(&e.args).unwrap_or_default(),
            e.archives
        ],
    )?;
    Ok(())
}

pub fn remove_emulator(conn: &Connection, name: &str) -> Result<usize, Error> {
    Ok(conn.execute("DELETE FROM emulators WHERE name = ?1", [name])?)
}

/// Every emulator romforge could use, and whether it is installed.
pub fn status(conn: &Connection, env: &Env) -> Result<Vec<EmulatorStatus>, Error> {
    let mut out = Vec::new();
    for u in user_emulators(conn)? {
        out.push(EmulatorStatus {
            found: env.which(&u.program).map(|p| p.display().to_string()),
            name: u.name,
            systems: vec![u.system],
        });
    }
    let ra = env.retroarch();
    let mut cores: Vec<&str> = systems::SYSTEMS
        .iter()
        .flat_map(|s| s.cores.iter().copied())
        .collect();
    cores.sort();
    cores.dedup();
    for core in cores {
        let found = match (&ra, env.core(core)) {
            (Some(r), Some(c)) => Some(format!("{} -L {}", describe(r), c.display())),
            _ => None,
        };
        out.push(EmulatorStatus {
            name: format!("retroarch:{core}"),
            systems: systems::SYSTEMS
                .iter()
                .filter(|s| s.cores.contains(&core))
                .map(|s| s.key.to_owned())
                .collect(),
            found,
        });
    }
    for e in KNOWN_EMULATORS {
        out.push(EmulatorStatus {
            name: e.name.to_owned(),
            systems: systems::SYSTEMS
                .iter()
                .filter(|s| s.emulators.contains(&e.name))
                .map(|s| s.key.to_owned())
                .collect(),
            found: env.known(e).map(|r| describe(&r)),
        });
    }
    Ok(out)
}

fn describe(r: &Runner) -> String {
    match r {
        Runner::Program(p) => p.display().to_string(),
        Runner::Flatpak(id) => format!("flatpak run {id}"),
    }
}

/// A matched file of a game, or one you named in Search: (game, system key,
/// path on disk, member).
fn candidates(conn: &Connection, query: &str) -> Result<Vec<(String, String, PathBuf, String)>, Error> {
    let as_path = std::fs::canonicalize(query).ok();
    let mut stmt = conn.prepare(
        concat!("SELECT name, key, path, member FROM (
         SELECT g.name AS name, s.key AS key, f.path AS path, f.member AS member
         FROM files f JOIN matches m ON m.file_id = f.file_id AND m.out_of_scope = 0
              AND m.via IN ", crate::report::strong!(), "
         JOIN roms r ON r.rom_id = m.rom_id JOIN games g ON g.game_id = r.game_id
         JOIN dats d ON d.dat_id = g.dat_id JOIN systems s ON s.system_id = d.system_id
         WHERE f.status = 'matched' AND (f.path = ?1 OR g.name = ?2 OR (?1 IS NULL AND g.name LIKE '%' || ?3 || '%' ESCAPE '\\'))
         UNION
         SELECT mi.title, s.key, f.path, f.member
         FROM files f JOIN manual_ids mi ON mi.sha1 = f.sha1 AND mi.size = f.size
         JOIN systems s ON s.system_id = mi.system_id
         WHERE f.status = 'manual' AND (f.path = ?1 OR mi.title = ?2 OR (?1 IS NULL AND mi.title LIKE '%' || ?3 || '%' ESCAPE '\\'))
         ) ORDER BY name <> ?2, name, path, member"),
    )?;
    let rows = stmt.query_map(
        params![
            as_path.map(|p| p.to_string_lossy().into_owned()),
            query,
            crate::like_escape(query)
        ],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                PathBuf::from(r.get::<_, String>(2)?),
                r.get(3)?,
            ))
        },
    )?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Work out how to start `query` (a game name, part of one, or a file path)
/// with `emulator` (a name from [`status`]) or the best installed one.
pub fn plan(conn: &Connection, env: &Env, query: &str, emulator: Option<&str>) -> Result<LaunchSpec, Error> {
    let found = candidates(conn, query)?;
    let Some((game, system_key, _, _)) = found.first().cloned() else {
        // A game that is a folder (ScummVM, DOS) by its title.
        let rows = crate::view::library(conn)?.rows;
        let q = query.to_lowercase();
        let folders: Vec<&crate::view::GameRow> = rows
            .iter()
            .filter(|r| r.folder.is_some() && r.playable)
            .filter(|r| r.title == query || r.title.to_lowercase().contains(&q))
            .collect();
        return match folders.as_slice() {
            [] => Err(Error::Launch(format!("no matched file for {query:?}"))),
            [one] => plan_row(conn, env, &one.id, emulator),
            many => match many.iter().find(|r| r.title == query) {
                Some(r) => plan_row(conn, env, &r.id, emulator),
                None => Err(Error::Launch(format!(
                    "{query:?} matches several games: {}",
                    many.iter()
                        .take(5)
                        .map(|r| r.title.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ")
                ))),
            },
        };
    };
    let titles: std::collections::BTreeSet<&str> = found.iter().map(|f| f.0.as_str()).collect();
    if titles.len() > 1 && !titles.contains(query) {
        let some: Vec<&str> = titles.iter().take(5).copied().collect();
        return Err(Error::Launch(format!(
            "{query:?} matches several games: {}",
            some.join(" | ")
        )));
    }
    let files: Vec<(PathBuf, String)> = found
        .iter()
        .filter(|f| f.0 == game)
        .map(|f| (f.2.clone(), f.3.clone()))
        .collect();
    start(conn, env, &game, &system_key, &files, emulator)
}

/// Work out how to start one row of the library (as [`crate::view`] lists
/// it): exactly its files, whatever else shares its title.
pub fn plan_row(
    conn: &Connection,
    env: &Env,
    row_id: &str,
    emulator: Option<&str>,
) -> Result<LaunchSpec, Error> {
    let v = crate::view::library(conn)?;
    let row = v
        .rows
        .iter()
        .find(|r| r.id == row_id)
        .ok_or_else(|| Error::Launch(format!("no library row {row_id:?}")))?;
    if !row.playable {
        return Err(Error::Launch(format!(
            "{} isn't identified yet (Search), so it isn't started",
            row.title
        )));
    }
    let system = row
        .system
        .clone()
        .ok_or_else(|| Error::Launch(format!("{} has no platform", row.title)))?;
    let mut stmt = conn.prepare_cached("SELECT path, member FROM files WHERE file_id = ?1")?;
    let mut files = Vec::new();
    for id in row.file_ids() {
        let (path, member): (String, String) = stmt.query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        files.push((PathBuf::from(path), member));
    }
    files.sort();
    if let Some(folder) = &row.folder {
        let folder = PathBuf::from(folder);
        // A DOS game in a folder starts from its program (DOSBox mounts the
        // folder around it); in an archive, from the archive. A folder of
        // its own (ScummVM) is handed over as the folder.
        let dos = systems::by_key(&system).is_some_and(|s| s.folders == systems::Folders::Zip);
        if folder.is_dir() {
            let program = files
                .iter()
                .find(|f| dos && f.1.is_empty() && crate::view::is_program(&f.0.to_string_lossy()))
                .cloned();
            files = vec![program.unwrap_or((folder, String::new()))];
        } else {
            files.retain(|f| f.0 == folder && !f.1.is_empty());
        }
    }
    start(conn, env, &row.title, &system, &files, emulator)
}

/// How to start `game` on `system_key` from `files` (path, archive member).
fn start(
    conn: &Connection,
    env: &Env,
    game: &str,
    system_key: &str,
    files: &[(PathBuf, String)],
    emulator: Option<&str>,
) -> Result<LaunchSpec, Error> {
    let game = game.to_owned();
    let system = systems::by_key(system_key).ok_or_else(|| Error::UnknownSystem(system_key.to_owned()))?;
    let Some(first) = files.first() else {
        return Err(Error::Launch(format!("no file for {game}")));
    };

    // The file to start: an archive (as a whole), a sheet, a CHD, else the
    // first file.
    let in_archive = files.iter().find(|f| !f.1.is_empty()).map(|f| f.0.clone());
    let is_sheet = |p: &Path| {
        p.extension().is_some_and(|e| {
            ["cue", "gdi", "m3u"].contains(&e.to_string_lossy().to_ascii_lowercase().as_str())
        })
    };
    let sheet = files
        .iter()
        .map(|f| f.0.clone())
        .find(|p| is_sheet(p))
        .or_else(|| own_sheet(&first.0, &is_sheet));
    let (rom, archived) = match (in_archive, sheet) {
        (Some(a), _) => {
            let chd = a.extension().is_some_and(|e| e.eq_ignore_ascii_case("chd"));
            (a, !chd)
        }
        (None, Some(s)) => (s, false),
        (None, None) => (first.0.clone(), false),
    };
    // One disk of a set: start the set's playlist, so disks can be swapped.
    let rom = playlist_for(&rom).unwrap_or(rom);

    // The folder the game is in (a folder game's own folder): its other
    // disks, tracks and saves are there.
    let grant = if rom.is_dir() {
        Some(rom.clone())
    } else {
        rom.parent().map(Path::to_path_buf)
    };
    let spec = |emulator: String, runner: &Runner, args: Vec<String>| {
        let (program, args) = command(runner, args, grant.as_deref());
        LaunchSpec {
            game: game.clone(),
            system: system.key.to_owned(),
            emulator,
            rom: rom.clone(),
            program,
            args,
        }
    };
    let wanted = |name: &str| emulator.is_none_or(|w| w.eq_ignore_ascii_case(name));
    let mut why_not = Vec::new();

    for u in user_emulators(conn)?
        .into_iter()
        .filter(|u| u.system == system.key)
    {
        if !wanted(&u.name) {
            continue;
        }
        if archived && !u.archives {
            why_not.push(format!("{} can't open archives", u.name));
            continue;
        }
        match env.which(&u.program) {
            Some(p) => return Ok(spec(u.name.clone(), &Runner::Program(p), fill(&u.args, &rom))),
            None => why_not.push(format!("{} ({}) not found", u.name, u.program)),
        }
    }
    if let Some(ra) = env.retroarch() {
        for core in system.cores {
            let name = format!("retroarch:{core}");
            if !wanted(&name) && !wanted("retroarch") {
                continue;
            }
            // A folder goes to a core as the `.scummvm` file in it.
            let content = if rom.is_dir() {
                match scummvm_file(&rom) {
                    Some(f) => f,
                    None => {
                        why_not.push(format!(
                            "{name}: no .scummvm file in the folder (import writes one)"
                        ));
                        continue;
                    }
                }
            } else {
                rom.clone()
            };
            match env.core(core) {
                Some(c) => {
                    let args = vec![
                        "-L".to_owned(),
                        c.to_string_lossy().into_owned(),
                        content.to_string_lossy().into_owned(),
                    ];
                    return Ok(spec(name, &ra, args));
                }
                None => why_not.push(format!("{name}: core not installed")),
            }
        }
    } else if !system.cores.is_empty() {
        why_not.push("RetroArch not found".into());
    }
    for name in system.emulators {
        let Some(e) = KNOWN_EMULATORS.iter().find(|e| e.name == *name) else {
            continue;
        };
        if !wanted(e.name) {
            continue;
        }
        if archived && !e.archives {
            why_not.push(format!("{} can't open archives", e.name));
            continue;
        }
        match env.known(e) {
            Some(r) => return Ok(spec(e.name.to_owned(), &r, fill(e.args, &rom))),
            None => why_not.push(format!("{} not installed", e.name)),
        }
    }
    if let Some(name) = emulator {
        let known = user_emulators(conn)?
            .iter()
            .any(|u| u.name.eq_ignore_ascii_case(name))
            || name.eq_ignore_ascii_case("retroarch")
            || system
                .cores
                .iter()
                .any(|c| name.eq_ignore_ascii_case(&format!("retroarch:{c}")))
            || system.emulators.iter().any(|e| name.eq_ignore_ascii_case(e));
        if !known {
            return Err(Error::Launch(format!(
                "{name:?} is not an emulator for {} (see `emulator list --all`)",
                system.key
            )));
        }
    }
    Err(Error::Launch(format!(
        "no emulator to start {game} [{}]: {}",
        system.key,
        if why_not.is_empty() {
            "none known for this system; add one with `emulator add`".to_owned()
        } else {
            why_not.join("; ")
        }
    )))
}

/// A sheet next to a loose track that lists it: the disc's own cue (or
/// gdi), when it differs from the DAT's and so didn't match.
fn own_sheet(track: &Path, is_sheet: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    let dir = track.parent()?;
    let mut sheets: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_sheet(p))
        .collect();
    sheets.sort();
    sheets.into_iter().find(|s| match crate::disc::sheet_files(s) {
        Some(Ok(listed)) => listed.iter().any(|f| f == track),
        _ => false,
    })
}

/// The `.scummvm` file in a game's folder (its game id, for RetroArch).
fn scummvm_file(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("scummvm")))
        .collect();
    found.sort();
    found.into_iter().next()
}

/// A playlist beside `disk` that lists it (import writes one per complete
/// multi-disk set): an `.m3u`, or np2kai's `.cmd` (a command line of
/// quoted disks).
fn playlist_for(disk: &Path) -> Option<PathBuf> {
    let dir = disk.parent()?;
    let mut lists: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("m3u") || e.eq_ignore_ascii_case("cmd"))
        })
        .collect();
    lists.sort();
    lists.into_iter().find(|l| {
        let Ok(text) = std::fs::read_to_string(l) else {
            return false;
        };
        let cmd = l.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd"));
        let entries: Vec<&str> = if cmd {
            text.split('"').skip(1).step_by(2).collect()
        } else {
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect()
        };
        entries.iter().any(|e| dir.join(e) == disk)
    })
}

/// ScummVM's detection list, from the installed ScummVM (on `PATH` or
/// Flatpak): `--dump-all-detection-entries`, which prints it and exits.
pub fn scummvm_list(env: &Env) -> Result<Vec<u8>, Error> {
    let (program, out) = run_scummvm(env, &["--dump-all-detection-entries".to_owned()], None)?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(Error::Launch(format!(
            "{} gave no detection list ({}); it needs a ScummVM with --dump-all-detection-entries",
            program.display(),
            out.status
        )));
    }
    Ok(out.stdout)
}

/// What the installed ScummVM finds in `root` and every folder below it
/// (`--detect --recursive`), as it prints it.
pub fn scummvm_detect(env: &Env, root: &Path) -> Result<String, Error> {
    let args = [
        "--detect".to_owned(),
        "--recursive".to_owned(),
        format!("--path={}", root.display()),
    ];
    let (_, out) = run_scummvm(env, &args, Some(root))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn run_scummvm(
    env: &Env,
    args: &[String],
    grant: Option<&Path>,
) -> Result<(PathBuf, std::process::Output), Error> {
    let e = KNOWN_EMULATORS
        .iter()
        .find(|e| e.name == "scummvm")
        .ok_or_else(|| Error::Launch("scummvm is not a known emulator".into()))?;
    let runner = env
        .known(e)
        .ok_or_else(|| Error::Launch("ScummVM isn't installed (on PATH or as a Flatpak)".into()))?;
    let (program, args) = command(&runner, args.to_vec(), grant);
    let out = std::process::Command::new(&program)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map_err(|e| Error::Launch(format!("{}: {e}", program.display())))?;
    Ok((program, out))
}

/// Start the emulator and return its process id. romforge doesn't wait for
/// it, but a thread reaps it when it exits so no zombie is left behind.
pub fn spawn(spec: &LaunchSpec) -> Result<u32, Error> {
    let mut child = std::process::Command::new(&spec.program)
        .args(&spec.args)
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| Error::Launch(format!("{}: {e}", spec.program.display())))?;
    let pid = child.id();
    std::thread::spawn(move || child.wait());
    Ok(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_emulators_cover_the_systems_table() {
        for s in systems::SYSTEMS {
            for e in s.emulators {
                assert!(
                    KNOWN_EMULATORS.iter().any(|k| k.name == *e),
                    "{} lists unknown {e}",
                    s.key
                );
            }
        }
        for e in KNOWN_EMULATORS {
            assert!(e.args.iter().any(|a| a.contains("{rom}")), "{}", e.name);
        }
    }

    #[test]
    fn flatpak_commands() {
        let (p, a) = command(&Runner::Flatpak("org.x.Y".into()), vec!["{rom}".into()], None);
        assert_eq!(p, PathBuf::from("flatpak"));
        assert_eq!(a, ["run", "org.x.Y", "{rom}"]);
        let (_, a) = command(
            &Runner::Flatpak("org.x.Y".into()),
            vec!["/m/g/a.zip".into()],
            Some(Path::new("/m/g")),
        );
        assert_eq!(a, ["run", "--filesystem=/m/g", "org.x.Y", "/m/g/a.zip"]);
        let (p, a) = command(
            &Runner::Program("/bin/x".into()),
            vec!["r".into()],
            Some(Path::new("/m")),
        );
        assert_eq!(
            (p, a),
            (PathBuf::from("/bin/x"), vec!["r".to_owned()]),
            "a plain program needs none"
        );
        assert_eq!(
            fill(&["-e", "{rom}"], Path::new("/r/a b.iso")),
            ["-e", "/r/a b.iso"]
        );
    }
}
