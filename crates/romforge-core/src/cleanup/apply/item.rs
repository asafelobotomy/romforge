//! One item of a cleanup: what the database is needed for (before and
//! after) on the calling thread, the writing itself on a worker.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::hash::{Hashes, hash_stream};
use crate::view::LibraryView;
use crate::{Error, layout};

use super::super::items::write_set;
use super::super::parts::{extras, open_part};
use super::super::write::{copy_folder, place, write_folder_zip, write_loose, write_zip};
use super::super::{Format, Item, Part};
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
    /// The new file's content (SHA-1, size) when it isn't hashed as what
    /// it was made from (a WUA): a name you gave the game moves to it.
    carried: Option<(String, u64)>,
}

/// Checks and reads for an item written on a worker.
pub(super) fn prepare(
    conn: &Connection,
    view: &LibraryView,
    item: &Item,
    staging: PathBuf,
) -> Result<Prepared, Error> {
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
        extras: extras(conn, view, &item.row_id)?,
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
            carried: None,
        });
    }
    let _ = std::fs::remove_file(part);
    let mut carried = None;
    let written = match item.format {
        Format::Zip => write_zip(item, extras, staging, part).map(|()| false),
        Format::Folder => write_folder_zip(item, extras, staging, part).map(|()| false),
        Format::Rvz | Format::Z3ds | Format::Wua => {
            super::super::compress::write_compressed(item, staging, part).map(|c| {
                carried = c;
                false
            })
        }
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
        // The sidecar just put in place is this attempt's own: it goes too,
        // or a later attempt would find it there.
        if let Some(side) = &item.sidecar {
            let _ = std::fs::remove_dir_all(side);
        }
        return Err(e);
    }
    Ok(Written {
        took_sheet,
        sidecar: item.sidecar.is_some(),
        carried,
    })
}

/// Journal what a worker is about to put in place, before it does: a
/// crash in between leaves rows undo can act on (a row for nothing is
/// dropped; a folder still marked `dir` goes to the trash whole), never a
/// file nothing knows about.
pub(super) fn journal_intent(conn: &Connection, batch: i64, item: &Item) -> Result<(), Error> {
    if item.format == Format::Dir {
        return journal_output(conn, batch, &item.target, "dir");
    }
    journal_output(conn, batch, &item.target, "file")?;
    if let Some(side) = &item.sidecar {
        journal_output(conn, batch, side, "dir")?;
    }
    Ok(())
}

/// The intent rows of an item that wasn't written after all.
pub(super) fn forget_intent(conn: &Connection, batch: i64, item: &Item) -> Result<(), Error> {
    for p in std::iter::once(&item.target).chain(&item.sidecar) {
        conn.execute(
            "DELETE FROM cleanup_outputs WHERE batch_id = ?1 AND path = ?2",
            params![batch, p.to_string_lossy()],
        )?;
    }
    Ok(())
}

/// Journal what a worker put in place, so undo can take it back: a folder
/// it wrote (a folder game, a sidecar) with what is in it, so undo takes
/// just that and leaves anything added since.
pub(super) fn journal(conn: &Connection, batch: i64, item: &Item, w: &Written) -> Result<(), Error> {
    // A name you gave it, kept for the new file (the old one's stays, for
    // undo).
    if let (Some((sha1, size)), Some(old)) = (&w.carried, item.parts.first()) {
        conn.execute(
            "INSERT OR REPLACE INTO manual_ids (sha1, size, system_id, title, created_at, base, relation, tag, proof)
             SELECT ?1, ?2, system_id, title, created_at, base, relation, tag, proof
             FROM manual_ids WHERE sha1 = ?3 AND size = ?4",
            params![sha1, *size as i64, old.sha1, old.size as i64],
        )?;
    }
    let dir = match (&item.format, w.sidecar, &item.sidecar) {
        (Format::Dir, _, _) => &item.target,
        (_, true, Some(side)) => side,
        _ => return Ok(()),
    };
    conn.execute(
        "UPDATE cleanup_outputs SET kind = 'setdir' WHERE batch_id = ?1 AND path = ?2",
        params![batch, dir.to_string_lossy()],
    )?;
    journal_tree(conn, batch, dir)
}

/// A set or loose game, written (and journaled) as it goes.
pub(super) fn write_here(
    conn: &Connection,
    view: &LibraryView,
    batch: i64,
    item: &Item,
    staging: &Path,
) -> Result<bool, Error> {
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
        extras(conn, view, &item.row_id).and_then(|extras| write_loose(conn, batch, item, &extras, staging));
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

/// A game already in `games` (Games/): before its copies outside go, its
/// target must still be there and every ROM of them still held there,
/// read now and checked by size and SHA1.
pub(super) fn still_held(conn: &Connection, item: &Item, games: &Path, staging: &Path) -> Result<(), Error> {
    if !item.target.is_file() {
        return Err(Error::Cleanup(format!(
            "{} is gone since the plan; its copies stay",
            item.target.display()
        )));
    }
    let mut at = conn.prepare_cached("SELECT path, member FROM files WHERE sha1 = ?1 AND size = ?2")?;
    let mut chds: HashMap<PathBuf, Vec<(String, Hashes)>> = HashMap::new();
    for c in item
        .copies
        .iter()
        .filter(|p| !p.name.to_lowercase().ends_with(".cue"))
    {
        let held: Vec<(String, String)> = at
            .query_map(params![c.sha1, c.size], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        let ok = held
            .into_iter()
            .filter(|(path, _)| Path::new(path).starts_with(games))
            .any(|(path, member)| {
                let path = PathBuf::from(path);
                let same = |h: &Hashes| h.raw.sha1 == c.sha1 && h.raw.size == c.size;
                if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("chd")) {
                    let members = chds
                        .entry(path.clone())
                        .or_insert_with(|| crate::disc::chd_members(&path).unwrap_or_default());
                    return members
                        .iter()
                        .any(|(n, h)| (member.is_empty() || *n == member) && same(h));
                }
                let part = Part {
                    member,
                    path,
                    ..c.clone()
                };
                open_part(&part, staging)
                    .and_then(|r| Ok(hash_stream(r, c.size)?))
                    .is_ok_and(|h| same(&h))
            });
        if !ok {
            return Err(Error::Cleanup(format!(
                "{} no longer holds {} as planned; its copies stay",
                games.display(),
                c.name
            )));
        }
    }
    Ok(())
}

impl Prepared {
    pub(super) fn staging(&self) -> &Path {
        &self.staging
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A disc whose CHD can't go in place (something appeared there)
    /// leaves no sidecar behind either.
    #[test]
    fn a_failed_place_takes_its_sidecar_back() {
        let dir = tempfile::tempdir().unwrap();
        let iso = dir.path().join("Game.iso");
        let data: Vec<u8> = (0..2048u32 * 4).map(|i| (i % 251) as u8).collect();
        std::fs::write(&iso, &data).unwrap();
        let out = dir.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        let target = out.join("Game.chd");
        std::fs::write(&target, b"appeared meanwhile").unwrap();
        let item = Item {
            row_id: "r".into(),
            title: "Game".into(),
            system: "ps2".into(),
            format: Format::ChdDvd,
            target: target.clone(),
            sidecar: Some(out.join("Game")),
            parts: vec![Part {
                name: "Game.iso".into(),
                path: iso,
                member: String::new(),
                size: data.len() as u64,
                sha1: crate::cleanup::sha1_of(&data[..]).unwrap(),
            }],
            trash: vec![],
            kept: vec![],
            copies: vec![],
            sheet: None,
            existing: false,
            folder: None,
            code: None,
        };
        let p = Prepared {
            staging: dir.path().join("staging"),
            dir: out.clone(),
            name: "Game.chd".into(),
            part: out.join(layout::part("Game.chd")),
            extras: vec![("Game.romforge.md".into(), b"# Game".to_vec())],
        };
        assert!(write(&item, &p).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"appeared meanwhile");
        let left: Vec<_> = std::fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, ["Game.chd"], "no sidecar, no part file");
    }
}
