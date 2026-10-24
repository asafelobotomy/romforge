//! A BIOS import's journal read back: which places hold a file you picked,
//! and undoing the last import (what it wrote goes, originals it moved to
//! the trash come back).

use std::collections::HashSet;
use std::path::PathBuf;

use rusqlite::Connection;

use crate::Error;

/// The places holding a name-only file you picked, from imports not undone.
pub(super) fn picked(conn: &Connection) -> Result<HashSet<PathBuf>, Error> {
    let mut stmt = conn.prepare(
        "SELECT o.path FROM firmware_outputs o JOIN firmware_batches b ON b.batch_id = o.batch_id
         WHERE o.picked = 1 AND b.undone_at IS NULL",
    )?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.filter_map(Result::ok).map(PathBuf::from).collect())
}

/// Take away what the last import not yet undone wrote. Returns how many
/// files went.
pub fn undo(conn: &Connection) -> Result<usize, Error> {
    let batch: Option<i64> = conn
        .query_row(
            "SELECT batch_id FROM firmware_batches WHERE undone_at IS NULL ORDER BY batch_id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();
    let Some(batch) = batch else {
        return Err(Error::Launch("no BIOS import to undo".into()));
    };
    let emptied: Option<String> = conn.query_row(
        "SELECT emptied_at FROM firmware_batches WHERE batch_id = ?1",
        [batch],
        |r| r.get(0),
    )?;
    if emptied.is_some() {
        return Err(Error::Launch(
            "the last BIOS import's originals were deleted with the trash: it can't be undone".into(),
        ));
    }
    super::originals::restore(conn, batch)?;
    let mut stmt = conn.prepare("SELECT path FROM firmware_outputs WHERE batch_id = ?1")?;
    let paths: Vec<String> = stmt.query_map([batch], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let mut n = 0;
    for p in &paths {
        match std::fs::remove_file(p) {
            Ok(()) => n += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    conn.execute(
        concat!(
            "UPDATE firmware_batches SET undone_at = ",
            crate::db::now!(),
            " WHERE batch_id = ?1"
        ),
        [batch],
    )?;
    Ok(n)
}
