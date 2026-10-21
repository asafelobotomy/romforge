//! One item of a cleanup: what the database is needed for (before and
//! after) on the calling thread, the writing itself on a worker.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::{Error, layout};

use super::super::items::write_set;
use super::super::parts::extras;
use super::super::write::{copy_folder, place, write_folder_zip, write_loose, write_zip};
use super::super::{Format, Item};
use super::{journal_output, journal_tree};

/// Sets and loose games write through the database as they go: they stay
/// on the calling thread. Everything else can be written on a worker.
pub(super) fn on_worker(item: &Item) -> bool {
    !item.existing && !matches!(item.format, Format::Set | Format::Loose)
}

/// What a worker needs, read beforehand.
pub(super) struct Prepared {
    staging: PathBuf,
    dir: PathBuf,
    name: String,
    part: PathBuf,
    extras: Vec<(String, Vec<u8>)>,
}

/// What a worker wrote and put in place.
pub(super) struct Written {
    pub took_sheet: bool,
    sidecar: bool,
}

/// Checks and reads for an item written on a worker.
pub(super) fn prepare(conn: &Connection, item: &Item, staging: PathBuf) -> Result<Prepared, Error> {
    if item.target.exists() {
        return Err(Error::Cleanup(format!(
            "{} already exists",
            item.target.display()
        )));
    }
    let dir = item.target.parent().unwrap_or(Path::new(".")).to_owned();
    std::fs::create_dir_all(&dir)?;
    let name = item
        .target
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let part = dir.join(layout::part(&name));
    Ok(Prepared {
        staging,
        extras: extras(conn, &item.row_id)?,
        dir,
        name,
        part,
    })
}

/// Write the item next to its target, check it, and move it into place
/// (no database). On failure nothing of it is left.
pub(super) fn write(item: &Item, p: &Prepared) -> Result<Written, Error> {
    let Prepared {
        staging,
        dir,
        name,
        part,
        extras,
    } = p;
    if item.format == Format::Dir {
        let _ = std::fs::remove_dir_all(part);
        if let Err(e) = copy_folder(item, extras, part).and_then(|()| place(part, &item.target)) {
            let _ = std::fs::remove_dir_all(part);
            return Err(e);
        }
        return Ok(Written {
            took_sheet: false,
            sidecar: false,
        });
    }
    let _ = std::fs::remove_file(part);
    let written = match item.format {
        Format::Zip => write_zip(item, extras, staging, part).map(|()| false),
        Format::Folder => write_folder_zip(item, extras, staging, part).map(|()| false),
        _ => super::super::chd::write_chd(item, staging, part),
    };
    let took_sheet = match written {
        Ok(t) => t,
        Err(e) => {
            let _ = std::fs::remove_file(part);
            return Err(e);
        }
    };
    // The sidecar is written to a temporary folder and renamed too.
    if let Some(side) = &item.sidecar {
        let tmp = dir.join(layout::side(name));
        let _ = std::fs::remove_dir_all(&tmp);
        let wrote = std::fs::create_dir_all(&tmp)
            .and_then(|()| {
                extras
                    .iter()
                    .try_for_each(|(f, data)| std::fs::write(tmp.join(f), data))
            })
            .map_err(Error::from)
            .and_then(|()| place(&tmp, side));
        if let Err(e) = wrote {
            let _ = std::fs::remove_dir_all(&tmp);
            let _ = std::fs::remove_file(part);
            return Err(e);
        }
    }
    if let Err(e) = place(part, &item.target) {
        let _ = std::fs::remove_file(part);
        return Err(e);
    }
    Ok(Written {
        took_sheet,
        sidecar: item.sidecar.is_some(),
    })
}

/// Journal what a worker put in place, so undo can take it back.
pub(super) fn journal(conn: &Connection, batch: i64, item: &Item, w: &Written) -> Result<(), Error> {
    if item.format == Format::Dir {
        return journal_tree(conn, batch, &item.target);
    }
    if let (true, Some(side)) = (w.sidecar, &item.sidecar) {
        journal_tree(conn, batch, side)?;
    }
    journal_output(conn, batch, &item.target, "file")
}

/// A set or loose game, written (and journaled) as it goes.
pub(super) fn write_here(conn: &Connection, batch: i64, item: &Item, staging: &Path) -> Result<bool, Error> {
    if item.target.exists() {
        return Err(Error::Cleanup(format!(
            "{} already exists",
            item.target.display()
        )));
    }
    let dir = item.target.parent().unwrap_or(Path::new("."));
    let new_dir = !dir.exists();
    std::fs::create_dir_all(dir)?;
    // The set's folder is journaled before what goes in it, so undo (last
    // first) empties it before removing it.
    if new_dir {
        journal_output(conn, batch, dir, "setdir")?;
    }
    if item.format == Format::Set {
        write_set(conn, batch, item)?;
        return Ok(false);
    }
    let written =
        extras(conn, &item.row_id).and_then(|extras| write_loose(conn, batch, item, &extras, staging));
    if let Err(e) = written {
        if new_dir && std::fs::remove_dir(dir).is_ok() {
            conn.execute(
                "DELETE FROM cleanup_outputs WHERE batch_id = ?1 AND path = ?2",
                params![batch, dir.to_string_lossy()],
            )?;
        }
        return Err(e);
    }
    Ok(false)
}

impl Prepared {
    pub(super) fn staging(&self) -> &Path {
        &self.staging
    }
}
