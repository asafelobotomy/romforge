//! Milestone 20: moving your game data into RomForge, when you ask: RPCS3's
//! devices, Cemu's MLC, xemu's disk and EEPROM, the 3DS emulators' NAND
//! and SD card ([`super::theirs`]). Guarded:
//! - a plan first: what goes where, how much, and whether it fits;
//! - nothing goes over files already there;
//! - every file is copied and checked byte for byte before any original
//!   is touched, and a failure removes what was copied;
//! - only then do the originals move to the RomForge trash
//!   (`.romforge-trash/move-<id>/<their full path>`), journaled, so undo
//!   puts them back until the trash is emptied;
//! - the emulator's romforge config then uses RomForge's copy
//!   ([`super::theirs::MOVED_IN`]).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use super::existing::UserDirs;
use super::theirs;
use crate::Error;
use crate::home::Home;

/// One folder or file to move.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DataItem {
    /// What it is (`dev_hdd0`, `MLC`, `NAND`, ...).
    pub what: String,
    pub from: PathBuf,
    pub to: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

/// What moving an emulator's data in would do.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct MovePlan {
    pub emulator: String,
    pub items: Vec<DataItem>,
    pub bytes: u64,
    /// Free space where it goes (none when it couldn't be read).
    pub free: Option<u64>,
    /// Why it can't be done now; `None`: it can.
    pub problem: Option<String>,
    /// Moved in already (and not undone).
    pub moved: bool,
}

/// The emulators whose data can be moved in.
pub fn movable(name: &str) -> bool {
    matches!(name, "rpcs3" | "cemu" | "xemu" | "azahar" | "lime3ds" | "citra")
}

/// (what, yours, RomForge's place for it) for `name`.
fn sources(name: &str, home: &Home, user: &UserDirs) -> Vec<(String, PathBuf, PathBuf)> {
    let dir = home.emulator_config(name);
    match name {
        "rpcs3" => theirs::rpcs3_devices(user)
            .into_iter()
            .map(|(dev, p)| (dev.to_owned(), p, dir.join("rpcs3").join(dev)))
            .collect(),
        "cemu" => theirs::cemu_mlc(user)
            .map(|m| ("MLC".to_owned(), m, dir.join("data/Cemu/mlc01")))
            .into_iter()
            .collect(),
        "xemu" => [
            ("hdd_path", "hard disk", "xbox_hdd.qcow2"),
            ("eeprom_path", "EEPROM", "eeprom.bin"),
        ]
        .into_iter()
        .filter_map(|(key, what, file)| {
            theirs::xemu_file(user, key).map(|p| (what.to_owned(), p, home.bios().join("xemu").join(file)))
        })
        .collect(),
        _ => super::azahar::folders(name)
            .and_then(|(_, reads)| theirs::citra_storage(user, reads))
            .map(|at| {
                let saves = home.saves().join("Nintendo 3DS");
                [("NAND", "nand"), ("SD card", "sdmc")]
                    .into_iter()
                    .filter(|(_, d)| at.join(d).exists())
                    .map(|(what, d)| (what.to_owned(), at.join(d), saves.join(d)))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Files and bytes under `p` (or `p` itself).
fn size_of(p: &Path) -> (usize, u64) {
    walkdir::WalkDir::new(p)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .fold((0, 0), |(n, b), e| {
            (n + 1, b + e.metadata().map_or(0, |m| m.len()))
        })
}

/// Free bytes on the drive holding `p` (or its nearest folder that exists).
fn free_at(p: &Path) -> Option<u64> {
    let at = p.ancestors().find(|a| a.exists())?;
    let s = rustix::fs::statvfs(at).ok()?;
    Some(s.f_bavail.saturating_mul(s.f_frsize))
}

/// What moving `name`'s data into RomForge would do; nothing is written.
pub fn plan(name: &str, home: &Home, user: Option<&UserDirs>) -> MovePlan {
    let mut out = MovePlan {
        emulator: name.to_owned(),
        moved: theirs::moved_in(&home.emulator_config(name)),
        ..MovePlan::default()
    };
    if !movable(name) {
        out.problem = Some(format!("{name} keeps no game data outside romforge's config"));
        return out;
    }
    let Some(user) = user else {
        out.problem = Some("your home folder isn't known".into());
        return out;
    };
    for (what, from, to) in sources(name, home, user) {
        let (files, bytes) = size_of(&from);
        out.bytes += bytes;
        out.items.push(DataItem {
            what,
            from,
            to,
            files,
            bytes,
        });
    }
    out.free = out.items.first().and_then(|i| free_at(&i.to));
    out.problem = if out.moved {
        Some("its data is in RomForge already".into())
    } else if out.items.is_empty() {
        Some("none of its data was found outside RomForge".into())
    } else if let Some(i) = out.items.iter().find(|i| i.to.exists() && size_of(&i.to).0 > 0) {
        Some(format!(
            "{} holds files already: nothing is ever moved over them",
            i.to.display()
        ))
    } else if out.free.is_some_and(|f| f < out.bytes) {
        Some("there isn't room for it where it goes".into())
    } else {
        None
    };
    out
}

/// Copy `from` (a file or folder) to `to`, each file checked; symbolic
/// links copied as links.
fn copy_all(from: &Path, to: &Path) -> Result<(), Error> {
    for e in walkdir::WalkDir::new(from).follow_links(false) {
        let e = e.map_err(std::io::Error::from)?;
        let rel = e.path().strip_prefix(from).unwrap_or(Path::new(""));
        let target = if rel.as_os_str().is_empty() {
            to.to_path_buf()
        } else {
            to.join(rel)
        };
        let kind = e.file_type();
        if kind.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if kind.is_symlink() {
                std::os::unix::fs::symlink(std::fs::read_link(e.path())?, &target)?;
            } else {
                crate::organize::copy_checked(e.path(), &target)?;
            }
        }
    }
    Ok(())
}

/// Move a file or folder, across drives if need be, never over anything.
fn move_any(from: &Path, to: &Path) -> Result<(), Error> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if from.is_dir() {
        match crate::organize::rename_noreplace(from, to) {
            Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
                crate::home::move_tree(from, to).map(|_| ())
            }
            r => r.map_err(Error::from),
        }
    } else {
        crate::organize::move_file(from, to)
            .map(|_| ())
            .map_err(Error::from)
    }
}

fn remove_any(p: &Path) -> std::io::Result<()> {
    match p.symlink_metadata() {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(p),
        Ok(_) => std::fs::remove_file(p),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Move `name`'s data into RomForge as [`plan`] says. Returns the move's id.
pub fn apply(conn: &Connection, name: &str, home: &Home, user: Option<&UserDirs>) -> Result<i64, Error> {
    let plan = plan(name, home, user);
    if let Some(why) = plan.problem {
        return Err(Error::Launch(format!("can't move {name}'s data: {why}")));
    }
    conn.execute(
        concat!(
            "INSERT INTO data_moves (emulator, created_at) VALUES (?1, ",
            crate::db::now!(),
            ")"
        ),
        [name],
    )?;
    let id = conn.last_insert_rowid();
    // Everything copied and checked first; on any failure the copies go.
    let mut made: Vec<&Path> = Vec::new();
    for i in &plan.items {
        conn.execute(
            "INSERT INTO data_move_items (move_id, original, copy) VALUES (?1, ?2, ?3)",
            params![id, i.from.to_string_lossy(), i.to.to_string_lossy()],
        )?;
        let fresh = !i.to.exists();
        if let Err(e) = copy_all(&i.from, &i.to) {
            if fresh {
                made.push(&i.to);
            }
            for p in made {
                let _ = remove_any(p);
            }
            conn.execute("DELETE FROM data_move_items WHERE move_id = ?1", [id])?;
            conn.execute("DELETE FROM data_moves WHERE move_id = ?1", [id])?;
            return Err(e);
        }
        if fresh {
            made.push(&i.to);
        }
    }
    // Then the originals, to the trash.
    for i in &plan.items {
        let rel = i.from.strip_prefix("/").unwrap_or(&i.from);
        let trashed = home.trash().join(format!("move-{id}")).join(rel);
        move_any(&i.from, &trashed)?;
        conn.execute(
            "UPDATE data_move_items SET trashed = ?1 WHERE move_id = ?2 AND original = ?3",
            params![trashed.to_string_lossy(), id, i.from.to_string_lossy()],
        )?;
    }
    let list: String = plan
        .items
        .iter()
        .map(|i| format!("{}\n", i.to.display()))
        .collect();
    crate::atomic::write(
        &home.emulator_config(name).join(theirs::MOVED_IN),
        list.as_bytes(),
    )?;
    Ok(id)
}

/// Take back the last move of `name`'s data: the originals come back from
/// the trash, RomForge's copies go. Returns how many items.
pub fn undo(conn: &Connection, name: &str, home: &Home) -> Result<usize, Error> {
    let (id, emptied): (i64, Option<String>) = conn
        .query_row(
            "SELECT move_id, emptied_at FROM data_moves WHERE emulator = ?1 AND undone_at IS NULL
             ORDER BY move_id DESC LIMIT 1",
            [name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| Error::Launch(format!("no move of {name}'s data to undo")))?;
    if emptied.is_some() {
        return Err(Error::Launch(format!(
            "{name}'s original data was deleted with the trash: the move can't be undone"
        )));
    }
    let items: Vec<(String, String, Option<String>)> = conn
        .prepare("SELECT original, copy, trashed FROM data_move_items WHERE move_id = ?1")?
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    for (original, copy, trashed) in &items {
        if let Some(t) = trashed {
            move_any(Path::new(t), Path::new(original))?;
        }
        remove_any(Path::new(copy))?;
    }
    let _ = std::fs::remove_file(home.emulator_config(name).join(theirs::MOVED_IN));
    conn.execute(
        concat!(
            "UPDATE data_moves SET undone_at = ",
            crate::db::now!(),
            " WHERE move_id = ?1"
        ),
        [id],
    )?;
    conn.execute(
        "UPDATE data_move_items SET trashed = NULL WHERE move_id = ?1",
        [id],
    )?;
    Ok(items.len())
}

#[cfg(test)]
mod tests;
