use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::{Error, layout};

use super::write::move_path;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct CleanupUndone {
    pub batch_id: i64,
    pub removed: usize,
    pub restored: usize,
    pub failed: Vec<String>,
}

/// Remove a batch's new files and put its originals back (the latest batch
/// by default). An original whose place is taken stays in the trash and is
/// reported.
pub fn undo(conn: &mut Connection, batch: Option<i64>) -> Result<CleanupUndone, Error> {
    let batch_id: i64 = match batch {
        Some(b) => b,
        None => conn
            .query_row(
                "SELECT max(batch_id) FROM cleanup_batches WHERE undone_at IS NULL AND emptied_at IS NULL",
                [],
                |r| r.get::<_, Option<i64>>(0),
            )?
            .ok_or_else(|| Error::Cleanup("nothing to undo".into()))?,
    };
    let emptied: Option<String> = conn
        .query_row(
            "SELECT emptied_at FROM cleanup_batches WHERE batch_id = ?1",
            [batch_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if let Some(when) = emptied {
        return Err(Error::Cleanup(format!(
            "batch #{batch_id}'s originals were deleted when the trash was emptied ({when}); it can't be undone"
        )));
    }
    let mut u = CleanupUndone {
        batch_id,
        ..CleanupUndone::default()
    };
    let trashed: Vec<(String, String)> = conn
        .prepare("SELECT original, trashed FROM cleanup_trash WHERE batch_id = ?1")?
        .query_map([batch_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (original, at) in trashed {
        // Journaled but never moved (a crash in between): nothing to do.
        if !Path::new(&at).exists() && Path::new(&original).exists() {
            conn.execute(
                "DELETE FROM cleanup_trash WHERE batch_id = ?1 AND trashed = ?2",
                params![batch_id, at],
            )?;
            continue;
        }
        match move_path(Path::new(&at), Path::new(&original)) {
            Ok(_) => {
                conn.execute(
                    "DELETE FROM cleanup_trash WHERE batch_id = ?1 AND trashed = ?2",
                    params![batch_id, at],
                )?;
                u.restored += 1;
                // Empty trash folders go too.
                let mut d = Path::new(&at).parent();
                while let Some(dir) = d {
                    if std::fs::remove_dir(dir).is_err() {
                        break;
                    }
                    d = dir.parent();
                    if dir.file_name().is_some_and(|n| n == layout::TRASH_DIR) {
                        break;
                    }
                }
            }
            Err(e) => u.failed.push(format!("{original}: {e}")),
        }
    }
    // While an original is still in the trash, the new files stay: they
    // may be the only other copy. Undo can be run again.
    if !u.failed.is_empty() {
        u.failed
            .push("the new files were kept, since not every original came back".into());
        return Ok(u);
    }
    let outputs: Vec<(String, String)> = conn
        .prepare("SELECT path, kind FROM cleanup_outputs WHERE batch_id = ?1 ORDER BY rowid DESC")?
        .query_map([batch_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (path, kind) in outputs {
        let p = Path::new(&path);
        let r = if kind == "dir" {
            std::fs::remove_dir_all(p)
        } else if kind == "setdir" {
            // A folder this batch made for a set: gone once it is empty;
            // anything put in it since stays.
            match std::fs::remove_dir(p) {
                Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => Ok(()),
                r => r,
            }
        } else {
            std::fs::remove_file(p)
        };
        match r {
            Ok(()) => {
                conn.execute(
                    "DELETE FROM cleanup_outputs WHERE batch_id = ?1 AND path = ?2",
                    params![batch_id, path],
                )?;
                u.removed += 1;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                conn.execute(
                    "DELETE FROM cleanup_outputs WHERE batch_id = ?1 AND path = ?2",
                    params![batch_id, path],
                )?;
            }
            Err(e) => u.failed.push(format!("{path}: {e}")),
        }
    }
    if u.failed.is_empty() {
        conn.execute(
            concat!(
                "UPDATE cleanup_batches SET undone_at = ",
                crate::db::now!(),
                " WHERE batch_id = ?1"
            ),
            [batch_id],
        )?;
    }
    Ok(u)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Batch {
    pub batch_id: i64,
    pub created_at: String,
    pub keep_originals: bool,
    pub outputs: i64,
    pub trashed: i64,
    pub undone_at: Option<String>,
    /// When its originals were deleted from the trash: it can't be undone.
    pub emptied_at: Option<String>,
}

pub fn batches(conn: &Connection) -> Result<Vec<Batch>, Error> {
    let mut stmt = conn.prepare(
        "SELECT b.batch_id, b.created_at, b.keep_originals, b.undone_at, b.emptied_at,
                (SELECT count(*) FROM cleanup_outputs o WHERE o.batch_id = b.batch_id AND o.kind = 'file'),
                (SELECT count(*) FROM cleanup_trash t WHERE t.batch_id = b.batch_id)
         FROM cleanup_batches b ORDER BY b.batch_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Batch {
            batch_id: r.get(0)?,
            created_at: r.get(1)?,
            keep_originals: r.get(2)?,
            undone_at: r.get(3)?,
            emptied_at: r.get(4)?,
            outputs: r.get(5)?,
            trashed: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}
