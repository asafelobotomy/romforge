//! Milestone 27, stage 1: an emulator's saves from the places it used until
//! now into `Saves/<system>/`, when you ask. Guarded like the game-data
//! move-in (`launch::move_in`):
//! - a plan first, every file and where it goes; nothing is written;
//! - nothing goes over a file there: one whose place is taken stays where
//!   it is, and the plan says so (the emulator then keeps saving there);
//! - a save whose system can't be told (by its game in the library) stays;
//! - every file moved is journaled, so undo puts it back;
//! - never while the emulator runs (the caller checks).
//!
//! Once its old place is empty the emulator saves in `Saves/` by itself.
//! The places, as in `launch/owned/saves.rs`:
//! - DuckStation `duckstation/memcards/`, PCSX2 `PCSX2/memcards/`, PPSSPP
//!   `ppsspp/PSP/SAVEDATA/` (in romforge's config for it), to one system;
//! - Dolphin `GC/<region>/Card A/` and `GC/MemoryCardA.<region>.raw`, to
//!   `Saves/GameCube/` (`<region>/`, the raw cards beside them);
//! - Flycast's VMUs in `data/flycast/`, to `Saves/Dreamcast/`;
//! - Mednafen `sav/<game>.<md5>.<ext>`, and mGBA's and melonDS's
//!   `<game>.sav` beside the game's file, to the game's system;
//! - ScummVM's own save folder (`$XDG_DATA_HOME/scummvm/saves/`, yours,
//!   outside RomForge: your own ScummVM then no longer finds them), to
//!   `Saves/ScummVM/`.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::Error;
use crate::home::Home;
use crate::launch::existing::UserDirs;
use crate::systems::{self, System};

/// One save file and where it goes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SaveMove {
    pub from: PathBuf,
    pub to: PathBuf,
    pub bytes: u64,
    /// A file is there already: this one stays where it is.
    pub taken: bool,
}

/// What gathering an emulator's saves would do.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct GatherPlan {
    pub emulator: String,
    pub moves: Vec<SaveMove>,
    /// Saves whose system couldn't be told: they stay.
    pub unplaced: Vec<PathBuf>,
    /// Why nothing can be moved now; `None`: it can.
    pub problem: Option<String>,
}

/// The emulators whose saves can be gathered.
pub fn gathers(name: &str) -> bool {
    matches!(
        name,
        "duckstation"
            | "pcsx2"
            | "ppsspp"
            | "dolphin"
            | "flycast"
            | "mednafen"
            | "mgba"
            | "melonds"
            | "scummvm"
    )
}

/// Every file under `dir`, with its path below it.
fn files_under(dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            let rel = e.path().strip_prefix(dir).unwrap_or(e.path()).to_path_buf();
            (e.path().to_path_buf(), rel)
        })
        .collect()
}

/// The library's game files on `name`'s systems (matched by hash, or named
/// by you), each with its system.
fn game_files(conn: &Connection, name: &str) -> Vec<(PathBuf, &'static System)> {
    let mine = |s: &System| {
        s.emulators
            .iter()
            .any(|n| crate::launch::known(n).is_some_and(|k| k.settings_name() == name))
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT f.path, d.system_id FROM files f
           JOIN matches m ON m.file_id = f.file_id JOIN roms r ON r.rom_id = m.rom_id
           JOIN games g ON g.game_id = r.game_id JOIN dats d ON d.dat_id = g.dat_id
          WHERE d.system_id IS NOT NULL
         UNION
         SELECT f.path, mi.system_id FROM files f
           JOIN manual_ids mi ON mi.sha1 = f.sha1 AND mi.size = f.size",
    ) else {
        return Vec::new();
    };
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)));
    rows.into_iter()
        .flatten()
        .flatten()
        .filter_map(|(p, id)| Some((PathBuf::from(p), systems::by_id(id).filter(|s| mine(s))?)))
        .collect()
}

/// (save, its system key) for `name`, or the save alone when its system
/// can't be told.
fn found(
    conn: &Connection,
    name: &str,
    home: &Home,
    user: Option<&UserDirs>,
) -> Vec<(PathBuf, Option<PathBuf>)> {
    let dir = home.emulator_config(name);
    let saves = home.saves();
    let into = |folder: &Path, to: PathBuf| -> Vec<(PathBuf, Option<PathBuf>)> {
        files_under(folder)
            .into_iter()
            .map(|(f, rel)| (f, Some(to.join(rel))))
            .collect()
    };
    match name {
        "duckstation" => into(&dir.join("duckstation/memcards"), saves.join("PlayStation")),
        "pcsx2" => into(&dir.join("PCSX2/memcards"), saves.join("PlayStation 2")),
        "ppsspp" => {
            let savedata = dir.join("ppsspp/PSP/SAVEDATA");
            if savedata.is_symlink() {
                return Vec::new();
            }
            into(&savedata, saves.join("PSP"))
        }
        "dolphin" => {
            let (gc, to) = (dir.join("GC"), saves.join("GameCube"));
            let mut out = Vec::new();
            for e in std::fs::read_dir(&gc)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
            {
                let n = e.file_name().to_string_lossy().into_owned();
                if e.path().is_dir() {
                    out.extend(into(&e.path().join("Card A"), to.join(&n)));
                } else if n.starts_with("MemoryCardA.") {
                    out.push((e.path(), Some(to.join(&n))));
                }
            }
            out
        }
        "flycast" => files_under(&dir.join("data/flycast"))
            .into_iter()
            .filter(|(_, rel)| {
                rel.parent() == Some(Path::new("")) && rel.to_string_lossy().contains("vmu_save")
            })
            .map(|(f, rel)| (f, Some(saves.join("Dreamcast").join(rel))))
            .collect(),
        "scummvm" => user
            .map(|u| into(&u.xdg.data.join("scummvm/saves"), saves.join("ScummVM")))
            .unwrap_or_default(),
        "mednafen" => {
            // `<game>.<md5>.<ext>`: the longest game name it starts with.
            let mut stems: HashMap<String, BTreeSet<&str>> = HashMap::new();
            for (p, s) in game_files(conn, name) {
                if let Some(st) = p.file_stem() {
                    stems
                        .entry(st.to_string_lossy().into_owned())
                        .or_default()
                        .insert(s.key);
                }
            }
            files_under(&dir.join("sav"))
                .into_iter()
                .filter(|(_, rel)| rel.parent() == Some(Path::new("")))
                .map(|(f, rel)| {
                    let n = rel.to_string_lossy().into_owned();
                    let system = n
                        .match_indices('.')
                        .rev()
                        .find_map(|(i, _)| stems.get(&n[..i]).filter(|s| s.len() == 1))
                        .and_then(|s| s.first().copied());
                    (f, system.map(|k| saves.join(k).join(&n)))
                })
                .collect()
        }
        "mgba" | "melonds" => {
            let mut seen = BTreeSet::new();
            game_files(conn, name)
                .into_iter()
                .filter_map(|(p, s)| {
                    let save = p.with_extension("sav");
                    let file = save.file_name()?.to_owned();
                    (save.is_file() && seen.insert(save.clone()))
                        .then(|| (save, Some(saves.join(s.key).join(file))))
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// What gathering `name`'s saves into `Saves/` would do; nothing is written.
pub fn plan(conn: &Connection, name: &str, home: &Home, user: Option<&UserDirs>) -> GatherPlan {
    let mut out = GatherPlan {
        emulator: name.to_owned(),
        ..GatherPlan::default()
    };
    if !gathers(name) {
        out.problem = Some(format!("{name} keeps no saves romforge gathers"));
        return out;
    }
    for (from, to) in found(conn, name, home, user) {
        match to {
            Some(to) => out.moves.push(SaveMove {
                bytes: from.metadata().map_or(0, |m| m.len()),
                taken: to.symlink_metadata().is_ok(),
                from,
                to,
            }),
            None => out.unplaced.push(from),
        }
    }
    if !out.moves.iter().any(|m| !m.taken) {
        out.problem = Some(if out.moves.is_empty() {
            "none of its saves are outside Saves/".into()
        } else {
            "every one has a file in its place in Saves/ already".into()
        });
    }
    out
}

/// Move `name`'s saves as [`plan`] says, each journaled. Returns the
/// gathering's id and how many files moved.
pub fn apply(
    conn: &Connection,
    name: &str,
    home: &Home,
    user: Option<&UserDirs>,
) -> Result<(i64, usize), Error> {
    let plan = plan(conn, name, home, user);
    if let Some(why) = plan.problem {
        return Err(Error::Launch(format!("can't gather {name}'s saves: {why}")));
    }
    conn.execute(
        concat!(
            "INSERT INTO save_gathers (emulator, created_at) VALUES (?1, ",
            crate::db::now!(),
            ")"
        ),
        [name],
    )?;
    let id = conn.last_insert_rowid();
    let mut n = 0;
    for m in plan.moves.iter().filter(|m| !m.taken) {
        if let Err(e) = crate::organize::move_file(&m.from, &m.to) {
            // One whose place filled meanwhile stays; anything else stops,
            // with what moved so far journaled for undo.
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                continue;
            }
            return Err(e.into());
        }
        conn.execute(
            "INSERT INTO save_gather_files (gather_id, original, moved_to) VALUES (?1, ?2, ?3)",
            params![id, m.from.to_string_lossy(), m.to.to_string_lossy()],
        )?;
        // Folders it leaves empty go, inside the emulator's config only.
        let config = home.emulator_config(name);
        crate::organize::remove_empty_parents(&m.from, |d| !d.starts_with(&config) || d == config);
        n += 1;
    }
    Ok((id, n))
}

/// Put the last gathering of `name`'s saves back where they were. Returns
/// how many files went back; one whose old place is taken stays in Saves/.
pub fn undo(conn: &Connection, name: &str) -> Result<usize, Error> {
    let id: i64 = conn
        .query_row(
            "SELECT gather_id FROM save_gathers WHERE emulator = ?1 AND undone_at IS NULL
             ORDER BY gather_id DESC LIMIT 1",
            [name],
            |r| r.get(0),
        )
        .map_err(|_| Error::Launch(format!("no gathering of {name}'s saves to undo")))?;
    let files: Vec<(String, String)> = conn
        .prepare("SELECT original, moved_to FROM save_gather_files WHERE gather_id = ?1")?
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut n = 0;
    for (original, moved) in &files {
        match crate::organize::move_file(Path::new(moved), Path::new(original)) {
            Ok(_) => n += 1,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::NotFound
                ) => {}
            Err(e) => return Err(e.into()),
        }
    }
    conn.execute(
        concat!(
            "UPDATE save_gathers SET undone_at = ",
            crate::db::now!(),
            " WHERE gather_id = ?1"
        ),
        [id],
    )?;
    Ok(n)
}

#[cfg(test)]
mod tests;
