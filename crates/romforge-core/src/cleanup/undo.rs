use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::home::Home;
use crate::organize::{remove_empty_parents, same_file};
use crate::{Error, layout};

use super::apply::{trash_path, trash_to};
use super::write::move_path;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct CleanupUndone {
    pub batch_id: i64,
    pub removed: usize,
    pub restored: usize,
    pub failed: Vec<String>,
}

/// Take a batch back: its originals out of the trash, its new files into
/// the trash (`.romforge-trash/undo-<batch>/`, so an undo can itself be
/// undone by hand until the trash is emptied). Batches go back newest
/// first only: one with a newer batch still standing (not undone, or
/// emptied) is refused, since that batch may have trashed the only other
/// copy of what this one wrote. The latest batch by default. An original
/// whose place is taken stays in the trash and is reported.
pub fn undo(conn: &mut Connection, batch: Option<i64>, home: Option<&Home>) -> Result<CleanupUndone, Error> {
    let latest: Option<i64> = conn.query_row(
        "SELECT max(batch_id) FROM cleanup_batches WHERE undone_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    let batch_id = match batch {
        Some(b) => {
            let undone: Option<Option<String>> = conn
                .query_row(
                    "SELECT undone_at FROM cleanup_batches WHERE batch_id = ?1",
                    [b],
                    |r| r.get(0),
                )
                .optional()?;
            match undone {
                None => return Err(Error::Cleanup(format!("there is no batch #{b}"))),
                Some(Some(when)) => {
                    return Err(Error::Cleanup(format!("batch #{b} was undone already ({when})")));
                }
                Some(None) => b,
            }
        }
        None => latest.ok_or_else(|| Error::Cleanup("nothing to undo".into()))?,
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
        let lead = if batch.is_none() { "nothing to undo: " } else { "" };
        return Err(Error::Cleanup(format!(
            "{lead}batch #{batch_id}'s originals were deleted when the trash was emptied ({when}); \
             it can't be undone, nor can any batch before it"
        )));
    }
    if let Some(newer) = latest.filter(|l| *l > batch_id) {
        return Err(Error::Cleanup(format!(
            "batch #{newer} came after batch #{batch_id} and isn't undone; batches are undone newest first"
        )));
    }
    let mut u = CleanupUndone {
        batch_id,
        ..CleanupUndone::default()
    };
    let aside = format!("undo-{batch_id}");
    let trashed: Vec<(String, String)> = conn
        .prepare("SELECT original, trashed FROM cleanup_trash WHERE batch_id = ?1")?
        .query_map([batch_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (original, at) in trashed {
        // A new file an undo cut short already set aside: it stays there.
        if set_aside_by_undo(Path::new(&at), &aside) {
            continue;
        }
        // Journaled but never moved (a crash in between): nothing to do.
        if !Path::new(&at).exists() && Path::new(&original).exists() {
            conn.execute(
                "DELETE FROM cleanup_trash WHERE batch_id = ?1 AND trashed = ?2",
                params![batch_id, at],
            )?;
            continue;
        }
        match restore(Path::new(&at), Path::new(&original)) {
            Ok(_) => {
                conn.execute(
                    "DELETE FROM cleanup_trash WHERE batch_id = ?1 AND trashed = ?2",
                    params![batch_id, at],
                )?;
                u.restored += 1;
                // Empty trash folders go too, the trash folder itself last.
                let at = Path::new(&at);
                let root = at
                    .ancestors()
                    .find(|d| d.file_name().is_some_and(|n| n == layout::TRASH_DIR))
                    .and_then(Path::parent);
                remove_empty_parents(at, |d| Some(d) == root);
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
    let setdirs: Vec<PathBuf> = outputs
        .iter()
        .filter(|(_, k)| k == "setdir")
        .map(|(p, _)| PathBuf::from(p))
        .collect();
    for (path, kind) in outputs {
        let p = Path::new(&path);
        let r = if kind == "setdir" {
            // A folder this batch made for a set: gone once it is empty;
            // anything put in it since stays.
            match std::fs::remove_dir(p) {
                Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => Ok(()),
                r => r.map_err(Error::from),
            }
        } else if p.symlink_metadata().is_err() {
            Err(Error::Io(std::io::ErrorKind::NotFound.into()))
        } else {
            // A file (or a folder journaled whole, without its files: an
            // older batch) goes to the trash, never deleted: it may have
            // changed since, or be the only copy left.
            trash_to(conn, batch_id, p, &aside_path(p, home, &setdirs, &aside))
        };
        match r {
            Ok(()) => {
                conn.execute(
                    "DELETE FROM cleanup_outputs WHERE batch_id = ?1 AND path = ?2",
                    params![batch_id, path],
                )?;
                u.removed += 1;
            }
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
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

/// Where undo sets a new file aside: in the RomForge folder's trash, else
/// in a trash folder beside the outermost folder the batch made around it
/// (never inside a folder undo then removes).
fn aside_path(path: &Path, home: Option<&Home>, setdirs: &[PathBuf], aside: &str) -> PathBuf {
    if home.is_some() {
        return trash_path(path, home, aside);
    }
    let top = path
        .ancestors()
        .skip(1)
        .filter(|a| setdirs.iter().any(|d| d == a))
        .last()
        .unwrap_or(path);
    let base = top.parent().unwrap_or(Path::new("."));
    let rel = path.strip_prefix(base).unwrap_or(path);
    base.join(layout::TRASH_DIR).join(aside).join(rel)
}

/// Whether a trash row is a new file undo set aside (under
/// `.romforge-trash/undo-<batch>/`), not an original.
fn set_aside_by_undo(at: &Path, aside: &str) -> bool {
    let c: Vec<_> = at.components().map(|c| c.as_os_str()).collect();
    c.windows(2).any(|w| w[0] == layout::TRASH_DIR && w[1] == aside)
}

/// Put a trashed original back. A move cut short (the copy back in place,
/// some of the trashed one left) is finished: what is left in the trash
/// goes where the same file is back, or moves where nothing is; anything
/// that differs stays and is reported.
fn restore(at: &Path, original: &Path) -> Result<(), Error> {
    match (at.symlink_metadata(), original.symlink_metadata()) {
        (Ok(a), Ok(o)) if a.is_dir() && o.is_dir() => crate::home::merge_tree(at, original).map(drop),
        (Ok(a), Ok(o)) if a.is_file() && o.is_file() && same_file(at, original)? => {
            Ok(std::fs::remove_file(at)?)
        }
        _ => Ok(move_path(at, original)?),
    }
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
