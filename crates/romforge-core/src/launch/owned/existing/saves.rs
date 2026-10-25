//! Milestone 20's leftover: RetroArch's own saves and states, brought in
//! with "Import my existing setup". romforge keeps one save folder and one
//! state folder per platform (`Saves/<platform>/`, `States/<platform>/`,
//! milestone 18, 1.3); the user's RetroArch keeps them where its
//! `retroarch.cfg` says (`savefile_directory`, `savestate_directory`),
//! flat or sorted into a folder per core (named by the core's library
//! name) or per content folder (`sort_savefiles_enable`,
//! `sort_savefiles_by_content_enable`, read in RetroArch 1.22.2's
//! `configuration.c` and `runloop.c`). Each file gets its platform from
//! the game it belongs to (its name is the content's, without the
//! extension), else from the core folder it is in when that core runs one
//! platform only. One whose platform can't be told stays where it is.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::home::Home;
use crate::systems;

/// The saves and states placed, and how many couldn't be.
#[derive(Debug, Default)]
pub(super) struct Placed {
    /// (theirs, romforge's place for it).
    pub pairs: Vec<(PathBuf, PathBuf)>,
    pub unplaced: usize,
}

/// `key`'s folder in `cfg` (`~/` the home folder, `:/` beside the file);
/// none when unset or `default` (next to the content: not findable).
fn folder(cfg: &str, key: &str, cfg_dir: &Path, home_dir: &Path) -> Option<PathBuf> {
    let v = cfg
        .lines()
        .find_map(|l| super::super::theirs::value_of(l, key, '='))?;
    let p = if let Some(rest) = v.strip_prefix("~/") {
        home_dir.join(rest)
    } else if let Some(rest) = v.strip_prefix(":/") {
        cfg_dir.join(rest)
    } else if v.starts_with('/') {
        PathBuf::from(v)
    } else {
        return None;
    };
    p.is_dir().then_some(p)
}

/// The content's name a save or state is named by: `Game.srm`,
/// `Game.state1`, `Game.state.auto`, `Game.state.png` are all `Game`.
fn content_name(file: &str) -> &str {
    match file.find(".state") {
        Some(i) => &file[..i],
        None => file.rsplit_once('.').map_or(file, |(s, _)| s),
    }
}

/// The name of a file without its extension.
fn stem(p: &str) -> String {
    let name = p.rsplit('/').next().unwrap_or(p);
    name.rsplit_once('.').map_or(name, |(s, _)| s).to_owned()
}

/// Library games' content names → the platforms they are on (matched by
/// hash, or named by you).
fn games(conn: &Connection) -> HashMap<String, BTreeSet<i64>> {
    let mut out: HashMap<String, BTreeSet<i64>> = HashMap::new();
    let Ok(mut stmt) = conn.prepare(
        "SELECT f.path, f.member, d.system_id FROM files f
           JOIN matches m ON m.file_id = f.file_id JOIN roms r ON r.rom_id = m.rom_id
           JOIN games g ON g.game_id = r.game_id JOIN dats d ON d.dat_id = g.dat_id
          WHERE d.system_id IS NOT NULL
         UNION
         SELECT f.path, f.member, mi.system_id FROM files f
           JOIN manual_ids mi ON mi.sha1 = f.sha1 AND mi.size = f.size",
    ) else {
        return out;
    };
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    });
    for (path, member, system) in rows.into_iter().flatten().flatten() {
        out.entry(stem(&path)).or_default().insert(system);
        if !member.is_empty() {
            out.entry(stem(&member)).or_default().insert(system);
        }
    }
    out
}

/// Core library names (the folders RetroArch sorts by) → the platforms
/// whose cores they are.
fn cores(db_dir: Option<&Path>) -> HashMap<String, BTreeSet<i64>> {
    let mut out: HashMap<String, BTreeSet<i64>> = HashMap::new();
    for s in systems::SYSTEMS {
        for core in s.cores {
            if let Some(name) = crate::obtain::library_name(db_dir, core) {
                out.entry(name).or_default().insert(s.id);
            }
        }
    }
    out
}

/// RetroArch's saves and states from the user's `cfg`, each with its place
/// in RomForge.
pub(super) fn retroarch(home: &Home, cfg: &Path, home_dir: &Path, conn: Option<&Connection>) -> Placed {
    let mut placed = Placed::default();
    let (Ok(text), Some(cfg_dir)) = (std::fs::read_to_string(cfg), cfg.parent()) else {
        return placed;
    };
    let saves = folder(&text, "savefile_directory", cfg_dir, home_dir);
    let states = folder(&text, "savestate_directory", cfg_dir, home_dir);
    if saves.is_none() && states.is_none() {
        return placed;
    }
    let games = conn.map(games).unwrap_or_default();
    let db_dir = conn
        .and_then(Connection::path)
        .map(Path::new)
        .and_then(Path::parent);
    let cores = cores(db_dir);
    let one = |set: Option<&BTreeSet<i64>>| match set {
        Some(s) if s.len() == 1 => s.iter().next().copied(),
        _ => None,
    };
    let mut seen = BTreeSet::new();
    for base in saves.iter().chain(&states) {
        for e in walkdir::WalkDir::new(base)
            .max_depth(3)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !e.file_type().is_file() || !seen.insert(e.path().to_path_buf()) {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let rel = e.path().strip_prefix(base).unwrap_or(e.path());
            // By its game, else by the core folder it is sorted into.
            let system = one(games.get(content_name(&name))).or_else(|| {
                rel.parent()
                    .into_iter()
                    .flat_map(Path::components)
                    .find_map(|c| one(cores.get(c.as_os_str().to_string_lossy().as_ref())))
            });
            let Some(system) = system.and_then(systems::by_id) else {
                placed.unplaced += 1;
                continue;
            };
            let state = name.contains(".state");
            let root = if state { home.states() } else { home.saves() };
            placed
                .pairs
                .push((e.path().to_path_buf(), root.join(system.key).join(&name)));
        }
    }
    placed
}
