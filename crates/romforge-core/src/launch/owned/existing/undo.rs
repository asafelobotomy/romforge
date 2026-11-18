//! Taking a setup import back. Nothing is deleted: the files it copied go
//! to the trash, since the emulator may have written to them since (a
//! memory card played on for weeks), and settings files it replaced come
//! back.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::Error;
use crate::home::Home;
use crate::scan::Stamp;

/// What undoing a setup import did.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SetupUndone {
    /// Files taken back (moved to the trash).
    pub files: usize,
    /// Of those, the ones changed since the import (or that can't be told,
    /// from an import before romforge noted it): saves made since.
    pub changed: Vec<PathBuf>,
    /// Where they went.
    pub trash: Option<PathBuf>,
}

/// Take back the last setup import of `emulator`: the files it copied go
/// to the trash, settings files it replaced come back.
pub fn undo(conn: &Connection, home: &Home, emulator: &str) -> Result<SetupUndone, Error> {
    let batch: i64 = conn
        .query_row(
            "SELECT import_id FROM setup_imports WHERE emulator = ?1 AND undone_at IS NULL
             ORDER BY import_id DESC LIMIT 1",
            [emulator],
            |r| r.get(0),
        )
        .map_err(|_| Error::Launch(format!("no setup import of {emulator} to undo")))?;
    type Row = (String, Option<String>, Option<u64>, Option<i64>);
    let rows: Vec<Row> = conn
        .prepare("SELECT path, backup, size, mtime FROM setup_import_files WHERE import_id = ?1")?
        .query_map([batch], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    let trash = home.trash().join(format!("setup-import-{batch}"));
    let mut done = SetupUndone::default();
    for (path, backup, size, mtime) in rows {
        let path = PathBuf::from(path);
        let Some(now) = Stamp::of(&path) else {
            // Gone already (deleted by hand): only its backup comes back.
            if let Some(b) = backup {
                std::fs::rename(&b, &path)?;
            }
            continue;
        };
        if (size, mtime) != (Some(now.size), Some(now.mtime)) {
            done.changed.push(path.clone());
        }
        to_trash(&path, &trash.join(path.strip_prefix("/").unwrap_or(&path)))?;
        done.files += 1;
        if let Some(b) = backup {
            std::fs::rename(&b, &path)?;
        }
    }
    done.trash = (done.files > 0).then_some(trash);
    conn.execute(
        concat!(
            "UPDATE setup_imports SET undone_at = ",
            crate::db::now!(),
            " WHERE import_id = ?1"
        ),
        [batch],
    )?;
    Ok(done)
}

/// Move one file to `to` in the trash, never over a file there; across
/// drives, copied and checked before the original goes.
fn to_trash(from: &Path, to: &Path) -> Result<(), Error> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match crate::organize::rename_noreplace(from, to) {
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            crate::organize::copy_checked(from, to)?;
            std::fs::remove_file(from)?;
            Ok(())
        }
        r => Ok(r?),
    }
}
