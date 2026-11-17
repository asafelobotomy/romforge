//! What a cleanup or import cut short (the app closed, the process killed)
//! leaves behind: its `batch-N` staging folder, the `.romforge-part` file
//! of the game it was writing, and games it put in place that the library
//! never scanned (the refresh after a batch didn't run), so the next
//! import found "already exists" and left their originals behind.
//!
//! A batch holds `batch-N/.lock` while it runs; [`clear`] removes only
//! what no running batch holds, and [`unscanned`] lists the games to scan
//! so the next plan finds them "already there" and takes their originals
//! as copies (each checked by size and SHA1 first, as always).

use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::{Error, layout};

const LOCK: &str = ".lock";

/// Held by a batch for its whole run: its staging folder is in use.
pub(crate) struct BatchLock(File);

impl BatchLock {
    pub(crate) fn take(staging_root: &Path) -> Result<BatchLock, Error> {
        std::fs::create_dir_all(staging_root)?;
        let f = File::create(staging_root.join(LOCK))?;
        f.lock()?;
        Ok(BatchLock(f))
    }

    /// Let go (the folder may already be gone).
    pub(crate) fn release(self) {
        let _ = self.0.unlock();
    }
}

/// What [`clear`] took away.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Recovered {
    /// Staging folders of batches that ended without cleaning up.
    pub staging: usize,
    /// Half-written games (`.romforge-part`) of those batches.
    pub parts: usize,
    /// Games those batches put in place, scanned now.
    pub scanned: usize,
    /// A batch is running now: nothing was cleared.
    pub busy: bool,
}

/// Whether a running batch holds `dir` (its lock is taken). A folder with
/// no lock, or one nobody holds, is left over.
fn held(dir: &Path) -> bool {
    let Ok(f) = File::open(dir.join(LOCK)) else {
        return false;
    };
    match f.try_lock() {
        Ok(()) => false,
        Err(TryLockError::WouldBlock) => true,
        // Can't tell: leave it.
        Err(TryLockError::Error(_)) => true,
    }
}

/// Remove the staging folders under `staging` that no running batch
/// holds and, when none runs, the `.romforge-part` beside each game a
/// batch meant to write but didn't finish.
pub(crate) fn clear(conn: &Connection, staging: &Path) -> Result<Recovered, Error> {
    let mut out = Recovered::default();
    let mut left = Vec::new();
    if let Ok(entries) = std::fs::read_dir(staging) {
        for e in entries.flatten() {
            let path = e.path();
            let is_batch = e.file_name().to_string_lossy().starts_with("batch-");
            if !is_batch || !e.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            if held(&path) {
                out.busy = true;
            } else {
                left.push(path);
            }
        }
    }
    if out.busy {
        return Ok(out);
    }
    for dir in left {
        std::fs::remove_dir_all(&dir)?;
        out.staging += 1;
    }
    for target in outputs(conn)? {
        if target.symlink_metadata().is_ok() {
            continue;
        }
        let (Some(dir), Some(name)) = (target.parent(), target.file_name()) else {
            continue;
        };
        let part = dir.join(layout::part(&name.to_string_lossy()));
        if part.is_file() {
            std::fs::remove_file(&part)?;
            out.parts += 1;
        }
    }
    Ok(out)
}

/// Games batches put in place (not undone) that the library has no row
/// for: they need a scan before the next plan.
pub(crate) fn unscanned(conn: &Connection) -> Result<Vec<PathBuf>, Error> {
    Ok(outputs(conn)?
        .into_iter()
        .filter(|p| p.is_file())
        .filter(|p| {
            conn.query_row(
                "SELECT 1 FROM files WHERE path = ?1",
                [p.to_string_lossy()],
                |_| Ok(()),
            )
            .is_err()
        })
        .collect())
}

/// Every game file the batches still standing wrote, or meant to: not
/// what is inside a folder they wrote (a sidecar's `.nfo` and details, a
/// set's files), which is journaled as `file` too.
fn outputs(conn: &Connection) -> Result<Vec<PathBuf>, Error> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT o.path FROM cleanup_outputs o
           JOIN cleanup_batches b ON b.batch_id = o.batch_id
          WHERE o.kind = 'file' AND b.undone_at IS NULL
            AND NOT EXISTS (
                SELECT 1 FROM cleanup_outputs d
                 WHERE d.batch_id = o.batch_id AND d.kind IN ('dir', 'setdir')
                   AND substr(o.path, 1, length(d.path) + 1) = d.path || '/')",
    )?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(PathBuf::from)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_batch_is_left_and_a_dead_one_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let dead = dir.path().join("batch-1");
        std::fs::create_dir_all(dead.join("0")).unwrap();
        std::fs::write(dead.join("0/a.part"), b"x").unwrap();
        let running = dir.path().join("batch-2");
        let lock = BatchLock::take(&running).unwrap();
        let r = clear(&conn, dir.path()).unwrap();
        assert!(r.busy && r.staging == 0, "{r:?}");
        assert!(dead.exists(), "nothing goes while a batch runs");
        drop(lock);
        let r = clear(&conn, dir.path()).unwrap();
        assert_eq!((r.staging, r.busy), (2, false), "{r:?}");
        assert!(!dead.exists() && !running.exists());
    }

    #[test]
    fn a_half_written_game_goes_and_a_written_one_is_listed() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO cleanup_batches (batch_id, created_at, keep_originals) VALUES (1, 'now', 0)",
            [],
        )
        .unwrap();
        let done = dir.path().join("Done.zip");
        let cut = dir.path().join("Cut.chd");
        std::fs::write(&done, b"zip").unwrap();
        std::fs::write(dir.path().join(layout::part("Cut.chd")), b"half").unwrap();
        for p in [&done, &cut] {
            crate::cleanup::journal_output(&conn, 1, p, "file").unwrap();
        }
        // A sidecar's details are journaled as files too; never a game.
        let side = dir.path().join("Done");
        std::fs::create_dir_all(&side).unwrap();
        std::fs::write(side.join("Done.nfo"), b"nfo").unwrap();
        crate::cleanup::journal_output(&conn, 1, &side, "dir").unwrap();
        crate::cleanup::journal_output(&conn, 1, &side.join("Done.nfo"), "file").unwrap();
        assert_eq!(unscanned(&conn).unwrap(), vec![done.clone()]);
        let r = clear(&conn, &dir.path().join("staging")).unwrap();
        assert_eq!(r.parts, 1);
        assert!(!dir.path().join(layout::part("Cut.chd")).exists());
        assert!(done.exists(), "a finished game stays");
    }
}
