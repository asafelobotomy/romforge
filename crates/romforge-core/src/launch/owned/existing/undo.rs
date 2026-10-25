//! Taking a setup import back.

use rusqlite::Connection;

use crate::Error;

/// Take back the last setup import of `emulator`: the files it copied go,
/// settings files it replaced come back. Returns how many files went.
pub fn undo(conn: &Connection, emulator: &str) -> Result<usize, Error> {
    let batch: i64 = conn
        .query_row(
            "SELECT import_id FROM setup_imports WHERE emulator = ?1 AND undone_at IS NULL
             ORDER BY import_id DESC LIMIT 1",
            [emulator],
            |r| r.get(0),
        )
        .map_err(|_| Error::Launch(format!("no setup import of {emulator} to undo")))?;
    let mut stmt = conn.prepare("SELECT path, backup FROM setup_import_files WHERE import_id = ?1")?;
    let rows: Vec<(String, Option<String>)> = stmt
        .query_map([batch], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut n = 0;
    for (path, backup) in rows {
        match std::fs::remove_file(&path) {
            Ok(()) => n += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        if let Some(b) = backup {
            std::fs::rename(&b, &path)?;
        }
    }
    conn.execute(
        concat!(
            "UPDATE setup_imports SET undone_at = ",
            crate::db::now!(),
            " WHERE import_id = ?1"
        ),
        [batch],
    )?;
    Ok(n)
}
