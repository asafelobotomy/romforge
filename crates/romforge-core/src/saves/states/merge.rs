//! The save states still in the old `States/<system>/` folders into
//! `Saves/<system>/states/`, when you ask. Guarded like gathering saves
//! ([`crate::saves::gather`], whose journal it shares under the name
//! [`NAME`]): a plan first; nothing goes over a file there (that one stays,
//! and its emulators keep writing to `States/` until it is empty); every
//! file moved is journaled, so undo puts it back; never while an emulator
//! that writes states there runs (the caller checks).

use std::path::Path;

use rusqlite::{Connection, params};

use crate::Error;
use crate::home::Home;
use crate::saves::gather::{GatherPlan, SaveMove};

/// The name its moves are journaled under (`save_gathers.emulator`).
pub const NAME: &str = "States";

/// What moving the old `States/` into `Saves/` would do; nothing is written.
pub fn plan(home: &Home) -> GatherPlan {
    let old = home.states();
    let mut out = GatherPlan {
        emulator: NAME.to_owned(),
        ..GatherPlan::default()
    };
    for (from, rel) in crate::launch::old_saves::files_under(&old) {
        let mut parts = rel.components();
        let Some(system) = parts.next() else { continue };
        let below = parts.as_path();
        if below.as_os_str().is_empty() {
            // A file loose in States/, of no system: it stays.
            out.unplaced.push(from);
            continue;
        }
        let to = home.saves().join(system).join(super::STATES).join(below);
        out.moves.push(SaveMove {
            bytes: from.metadata().map_or(0, |m| m.len()),
            taken: to.symlink_metadata().is_ok(),
            from,
            to,
        });
    }
    if !out.moves.iter().any(|m| !m.taken) {
        out.problem = Some(if out.moves.is_empty() {
            "no save states are left in States/".into()
        } else {
            "every one has a file in its place in Saves/ already".into()
        });
    }
    out
}

/// Move them as [`plan`] says, each journaled: (journal id, files moved).
pub fn apply(conn: &Connection, home: &Home) -> Result<(i64, usize), Error> {
    let plan = plan(home);
    if let Some(why) = plan.problem {
        return Err(Error::Launch(format!("can't move the save states: {why}")));
    }
    conn.execute(
        concat!(
            "INSERT INTO save_gathers (emulator, created_at) VALUES (?1, ",
            crate::db::now!(),
            ")"
        ),
        [NAME],
    )?;
    let id = conn.last_insert_rowid();
    let old = home.states();
    let mut n = 0;
    for m in plan.moves.iter().filter(|m| !m.taken) {
        conn.execute(
            "INSERT INTO save_gather_files (gather_id, original, moved_to) VALUES (?1, ?2, ?3)",
            params![id, m.from.to_string_lossy(), m.to.to_string_lossy()],
        )?;
        let row = conn.last_insert_rowid();
        if let Err(e) = crate::organize::move_file(&m.from, &m.to) {
            if crate::organize::source_left(&e) {
                return Err(e.into());
            }
            conn.execute("DELETE FROM save_gather_files WHERE rowid = ?1", [row])?;
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                continue;
            }
            return Err(e.into());
        }
        // Folders it leaves empty go, inside States/ (which stays).
        crate::organize::remove_empty_parents(&m.from, |d: &Path| !d.starts_with(&old) || d == old);
        n += 1;
    }
    Ok((id, n))
}

/// Put the last move back into `States/`: how many files went back.
pub fn undo(conn: &Connection) -> Result<usize, Error> {
    crate::saves::gather::undo(conn, NAME)
}
