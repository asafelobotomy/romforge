use std::path::Path;

use rusqlite::{Connection, params};

use crate::organize::remove_empty_parents;
use crate::{Error, layout};

/// One batch's originals waiting in the trash.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TrashBatch {
    pub batch_id: i64,
    pub created_at: String,
    pub files: usize,
    pub bytes: u64,
}

/// What emptying the trash removes (or removed).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct TrashSummary {
    pub batches: Vec<TrashBatch>,
    pub files: usize,
    pub bytes: u64,
    /// What couldn't be deleted (left in the trash).
    pub failed: Vec<String>,
}

/// Files under a path (a trashed file, or a trashed folder game), and
/// their size.
pub(super) fn on_disk(p: &Path) -> (usize, u64) {
    walkdir::WalkDir::new(p)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .fold((0, 0), |(n, b), e| {
            (n + 1, b + e.metadata().map_or(0, |m| m.len()))
        })
}

/// The trash's batches (not undone, not emptied), with what each holds:
/// all of them, or those made more than `older_than_days` ago.
pub fn trash_contents(conn: &Connection, older_than_days: Option<u32>) -> Result<TrashSummary, Error> {
    let cutoff = older_than_days.map(|d| format!("-{d} days"));
    let ids: Vec<(i64, String)> = conn
        .prepare(
            "SELECT b.batch_id, b.created_at FROM cleanup_batches b
             WHERE b.emptied_at IS NULL
               AND EXISTS (SELECT 1 FROM cleanup_trash t WHERE t.batch_id = b.batch_id)
               AND (?1 IS NULL OR b.created_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?1))
             ORDER BY b.batch_id",
        )?
        .query_map([cutoff], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut out = TrashSummary::default();
    let mut paths = conn.prepare("SELECT trashed FROM cleanup_trash WHERE batch_id = ?1")?;
    for (batch_id, created_at) in ids {
        let (mut files, mut bytes) = (0, 0);
        for p in paths.query_map([batch_id], |r| r.get::<_, String>(0))? {
            let (n, b) = on_disk(Path::new(&p?));
            files += n;
            bytes += b;
        }
        out.files += files;
        out.bytes += bytes;
        out.batches.push(TrashBatch {
            batch_id,
            created_at,
            files,
            bytes,
        });
    }
    Ok(out)
}

/// Delete the originals [`trash_contents`] lists, for good. Each batch is
/// marked emptied first, so it can't be undone from then on (its new files
/// would go with nothing to put back); what can't be deleted stays listed
/// in `failed`, and in the journal.
pub fn empty_trash(conn: &Connection, older_than_days: Option<u32>) -> Result<TrashSummary, Error> {
    let mut out = trash_contents(conn, older_than_days)?;
    for b in &out.batches {
        conn.execute(
            concat!(
                "UPDATE cleanup_batches SET emptied_at = ",
                crate::db::now!(),
                " WHERE batch_id = ?1"
            ),
            [b.batch_id],
        )?;
        let rows: Vec<String> = conn
            .prepare("SELECT trashed FROM cleanup_trash WHERE batch_id = ?1")?
            .query_map([b.batch_id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for at in rows {
            let p = Path::new(&at);
            let r = if p.is_dir() {
                std::fs::remove_dir_all(p)
            } else {
                std::fs::remove_file(p)
            };
            match r {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    out.failed.push(format!("{at}: {e}"));
                    continue;
                }
            }
            conn.execute(
                "DELETE FROM cleanup_trash WHERE batch_id = ?1 AND trashed = ?2",
                params![b.batch_id, at],
            )?;
            // The batch's folders in the trash go once empty.
            remove_empty_parents(p, |d| d.file_name().is_some_and(|n| n == layout::TRASH_DIR));
        }
    }
    Ok(out)
}
