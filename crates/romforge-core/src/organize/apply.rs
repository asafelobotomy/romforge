use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::Error;

use super::fs::{move_file, remove_empty_parents, same_file, source_left};
use super::{Applied, Batch, Plan, Undone};

/// Carry out a plan, journaling each move so [`undo`] can reverse it. A
/// plan that moves nothing leaves no batch behind (`batch_id` 0), so `undo`
/// never lands on an empty batch. `progress(done, total)` follows each
/// move; asked to stop, it stops between moves (`stopped`).
pub fn apply(
    conn: &mut Connection,
    plan: &Plan,
    dest: &Path,
    cancel: &crate::job::Cancel,
    mut progress: impl FnMut(usize, usize),
) -> Result<Applied, Error> {
    let mut applied = Applied::default();
    if plan.moves.is_empty() {
        return Ok(applied);
    }
    let dest = absolute_dest(dest);
    conn.execute(
        concat!(
            "INSERT INTO organize_batches (created_at, dest)
         VALUES (",
            crate::db::now!(),
            ", ?1)"
        ),
        [dest.to_string_lossy()],
    )?;
    let batch_id = conn.last_insert_rowid();
    // Moves whose journal row stays though they failed (the file is in
    // both places): the batch stays for undo.
    let mut kept_rows = 0;
    for (seq, m) in plan.moves.iter().enumerate() {
        progress(seq, plan.moves.len());
        if cancel.is_set() {
            applied.stopped = true;
            break;
        }
        // Journaled before it moves: a crash or a database error in between
        // leaves a row undo can act on, never a moved file nothing knows
        // about.
        let size = match std::fs::metadata(&m.from) {
            Ok(md) => md.len(),
            Err(e) => {
                applied.failed.push((m.clone(), e.to_string()));
                continue;
            }
        };
        conn.execute(
            "INSERT INTO organize_moves (batch_id, seq, src, dst, size) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                batch_id,
                seq as i64,
                m.from.to_string_lossy(),
                m.to.to_string_lossy(),
                size
            ],
        )?;
        match move_file(&m.from, &m.to) {
            Ok(_) => {
                let tx = conn.transaction()?;
                follow(&tx, &m.from, &m.to, &dest)?;
                tx.commit()?;
                applied.moved += 1;
            }
            // The copy is in place but the original stays: the row stays
            // too, so undo can remove the copy.
            Err(e) if source_left(&e) => {
                kept_rows += 1;
                applied.failed.push((m.clone(), e.to_string()));
            }
            Err(e) => {
                conn.execute(
                    "DELETE FROM organize_moves WHERE batch_id = ?1 AND seq = ?2",
                    params![batch_id, seq as i64],
                )?;
                applied.failed.push((m.clone(), e.to_string()));
            }
        }
    }
    if applied.moved == 0 && kept_rows == 0 {
        conn.execute("DELETE FROM organize_batches WHERE batch_id = ?1", [batch_id])?;
    } else {
        applied.batch_id = batch_id;
    }
    Ok(applied)
}

/// The library follows a moved file: its rows get the new path and move to
/// the scan root that contains it (`new_root` is added as one if none
/// does), so rescanning the folder it came from doesn't drop it.
pub(super) fn follow(
    tx: &rusqlite::Transaction,
    from: &Path,
    to: &Path,
    new_root: &Path,
) -> Result<(), Error> {
    let roots: Vec<(i64, String)> = tx
        .prepare("SELECT root_id, path FROM scan_roots")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let root = roots
        .iter()
        .filter(|(_, p)| to.starts_with(p))
        .max_by_key(|(_, p)| p.len())
        .map(|(id, _)| *id);
    let root = match root {
        Some(id) => id,
        None => {
            let dir = new_root;
            tx.query_row(
                concat!(
                    "INSERT INTO scan_roots (path, scanned_at)
                 VALUES (?1, ",
                    crate::db::now!(),
                    ") RETURNING root_id"
                ),
                [dir.to_string_lossy()],
                |r| r.get(0),
            )?
        }
    };
    tx.execute(
        "UPDATE files SET path = ?2, root_id = ?3 WHERE path = ?1",
        params![from.to_string_lossy(), to.to_string_lossy(), root],
    )?;
    Ok(())
}

pub(super) fn absolute_dest(dest: &Path) -> PathBuf {
    std::fs::canonicalize(dest)
        .or_else(|_| std::path::absolute(dest))
        .unwrap_or_else(|_| dest.to_path_buf())
}

/// Move the files of a batch (the latest one not yet undone, by default)
/// back where they were, newest first. A file that changed size since, or
/// whose old place is taken, stays put and is reported.
pub fn undo(conn: &mut Connection, batch_id: Option<i64>) -> Result<Undone, Error> {
    let batch_id = match batch_id {
        Some(id) => conn
            .query_row(
                "SELECT batch_id FROM organize_batches WHERE batch_id = ?1 AND undone_at IS NULL",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::Organize(format!("no batch {id} to undo")))?,
        None => conn
            .query_row(
                "SELECT max(batch_id) FROM organize_batches WHERE undone_at IS NULL",
                [],
                |r| r.get::<_, Option<i64>>(0),
            )?
            .ok_or_else(|| Error::Organize("nothing to undo".into()))?,
    };
    let dest: PathBuf = conn
        .query_row(
            "SELECT dest FROM organize_batches WHERE batch_id = ?1",
            [batch_id],
            |r| r.get::<_, String>(0),
        )?
        .into();
    let moves: Vec<(i64, String, String, i64)> = conn
        .prepare("SELECT seq, src, dst, size FROM organize_moves WHERE batch_id = ?1 ORDER BY seq DESC")?
        .query_map([batch_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    let mut undone = Undone {
        batch_id,
        ..Undone::default()
    };
    for (seq, src, dst, size) in moves {
        let (src, dst) = (PathBuf::from(src), PathBuf::from(dst));
        // Journaled but never moved (a crash in between): nothing to do.
        if !dst.exists() && src.exists() {
            conn.execute(
                "DELETE FROM organize_moves WHERE batch_id = ?1 AND seq = ?2",
                params![batch_id, seq],
            )?;
            continue;
        }
        let now = std::fs::metadata(&dst).map(|m| m.len() as i64);
        // The move copied but couldn't remove the original (or undo was cut
        // short): when both are the same file, the copy just goes.
        let both_same = src.exists() && dst.exists() && same_file(&src, &dst).unwrap_or(false);
        let result = match now {
            Ok(_) if both_same => std::fs::remove_file(&dst).map_err(|e| format!("{}: {e}", dst.display())),
            Ok(n) if n != size => Err(format!("{} changed size since it was moved", dst.display())),
            Ok(_) => move_file(&dst, &src).map(drop).map_err(|e| e.to_string()),
            Err(e) => Err(format!("{}: {e}", dst.display())),
        };
        match result {
            Ok(()) => {
                let tx = conn.transaction()?;
                tx.execute(
                    "DELETE FROM organize_moves WHERE batch_id = ?1 AND seq = ?2",
                    params![batch_id, seq],
                )?;
                follow(&tx, &dst, &src, src.parent().unwrap_or(&src))?;
                tx.commit()?;
                remove_empty_dirs(&dst, &dest);
                undone.restored += 1;
            }
            Err(e) => undone.failed.push((dst, e)),
        }
    }
    if undone.failed.is_empty() {
        conn.execute(
            concat!(
                "UPDATE organize_batches SET undone_at = ",
                crate::db::now!(),
                " WHERE batch_id = ?1"
            ),
            [batch_id],
        )?;
    }
    Ok(undone)
}

pub fn batches(conn: &Connection) -> Result<Vec<Batch>, Error> {
    let mut stmt = conn.prepare(
        "SELECT b.batch_id, b.created_at, b.dest, b.undone_at,
                (SELECT count(*) FROM organize_moves m WHERE m.batch_id = b.batch_id)
         FROM organize_batches b ORDER BY b.batch_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Batch {
            batch_id: r.get(0)?,
            created_at: r.get(1)?,
            dest: r.get(2)?,
            undone_at: r.get(3)?,
            moves: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Folders under `dest` that undo left empty, up to the first that still
/// has something in it. Never `dest` itself or anything outside it, and only
/// ever empty folders.
pub(super) fn remove_empty_dirs(moved_out_of: &Path, dest: &Path) {
    remove_empty_parents(moved_out_of, |d| d == dest || !d.starts_with(dest));
}
