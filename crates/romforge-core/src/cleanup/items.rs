use std::collections::BTreeSet;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::organize::sanitize;
use crate::view::{self};
use crate::{Error, layout, systems};

use super::apply::journal_output;
use super::write::{copy_hashed, sha1_of, zip_order};
use super::{Format, Item, Part, Source};

/// Whether a DAT's ROM name is safe as a path inside an archive: relative,
/// no `.` or `..` parts, no backslashes or drive letters, and not in the
/// `romforge/` folder kept for art and details.
pub(crate) fn safe_member_name(name: &str) -> bool {
    let parts: Vec<&str> = name.split('/').collect();
    !name.is_empty()
        && !name.contains('\\')
        && !name.contains(':')
        && parts.iter().all(|p| !p.is_empty() && *p != "." && *p != "..")
        && !parts[0].eq_ignore_ascii_case(layout::EXTRAS_DIR)
}

/// Every file in a zip or 7z archive.
pub(super) fn archive_names(path: &Path) -> Result<Vec<String>, Error> {
    Ok(archive_entries(path)?.into_iter().map(|(n, _)| n).collect())
}

/// Every file in a zip or 7z archive, with its size.
pub(super) fn archive_entries(path: &Path) -> Result<Vec<(String, u64)>, Error> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    match ext.as_deref() {
        Some("zip") => {
            let mut zip = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
            let mut out = Vec::new();
            for i in 0..zip.len() {
                let m = zip.by_index_raw(i)?;
                if !m.is_dir() {
                    out.push((m.name()?.into_owned(), m.size()));
                }
            }
            Ok(out)
        }
        Some("7z") => {
            let a = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())?;
            Ok(a.archive()
                .files
                .iter()
                .filter(|e| !e.is_directory())
                .map(|e| (e.name().to_owned(), e.size()))
                .collect())
        }
        _ => Err(Error::Cleanup(format!(
            "{} isn't an archive romforge reads",
            path.display()
        ))),
    }
}

/// The import of a game that is a folder: every file in it (or in the
/// archive holding it) under its own path. `Ok(Err(why))` when it can't
/// go; `Ok(Ok(None))` when it is in RomForge already or outside what is
/// imported.
pub(super) fn folder_item(
    conn: &Connection,
    row: &view::GameRow,
    system: &systems::System,
    place: &Path,
    dest: &Path,
    source: &Source,
    games: Option<&Path>,
) -> Result<Result<Option<Item>, String>, Error> {
    if games.is_some_and(|g| place.starts_with(g)) || !source.contains(place) {
        return Ok(Ok(None));
    }
    let mut parts = Vec::new();
    if place.is_dir() {
        for e in walkdir::WalkDir::new(place)
            .follow_links(false)
            .sort_by_file_name()
        {
            let e = e.map_err(std::io::Error::from)?;
            if !e.file_type().is_file() {
                continue;
            }
            let rel = e.path().strip_prefix(place).unwrap_or(e.path());
            parts.push(Part {
                name: rel.to_string_lossy().replace('\\', "/"),
                path: e.path().to_owned(),
                member: String::new(),
                size: e.metadata().map_err(std::io::Error::from)?.len(),
                sha1: String::new(),
            });
        }
    } else {
        for (name, size) in archive_entries(place)? {
            parts.push(Part {
                name: name.clone(),
                path: place.to_owned(),
                member: name,
                size,
                sha1: String::new(),
            });
        }
    }
    if parts.is_empty() {
        return Ok(Err("the folder is empty".into()));
    }
    if let Some(bad) = parts.iter().find(|p| !safe_member_name(&p.name)) {
        return Ok(Err(format!(
            "it holds {:?}, which can't be imported safely",
            bad.name
        )));
    }
    let name = sanitize(&row.title);
    let dir = dest.join(sanitize(system.key));
    let (format, target) = match system.folders {
        systems::Folders::Kept if place.is_dir() => (Format::Dir, dir.join(&name)),
        systems::Folders::Kept => {
            return Ok(Err("ScummVM games are imported as folders, not archives".into()));
        }
        _ => (Format::Folder, dir.join(format!("{name}.zip"))),
    };
    let code = match row.id.strip_prefix("d:").and_then(|id| id.parse::<i64>().ok()) {
        Some(id) => Some(conn.query_row(
            "SELECT code FROM folder_games WHERE folder_id = ?1",
            [id],
            |r| r.get(0),
        )?),
        None => None,
    };
    Ok(Ok(Some(Item {
        row_id: row.id.clone(),
        title: row.title.clone(),
        system: system.key.to_owned(),
        format,
        target,
        sidecar: None,
        parts,
        trash: Vec::new(),
        kept: Vec::new(),
        copies: Vec::new(),
        sheet: None,
        existing: false,
        folder: Some(place.to_owned()),
        code,
    })))
}

/// The import of an arcade set: the files its ROMs and disks are in (its
/// own zip, its parent's and BIOS's, CHDs in a folder named after a set),
/// copied whole under their own names into `Games/<System>/`. A file an
/// earlier game of the batch takes, or one already there, isn't copied
/// again. `Ok(Ok(None))`: nothing outside RomForge to take.
pub(super) fn set_item(
    conn: &Connection,
    row: &view::GameRow,
    system: &systems::System,
    dest: &Path,
    source: &Source,
    games: Option<&Path>,
    taken: &mut BTreeSet<PathBuf>,
) -> Result<Result<Option<Item>, String>, Error> {
    let dir = dest.join(sanitize(system.key));
    let mut stmt = conn.prepare_cached("SELECT path, member FROM files WHERE file_id = ?1")?;
    let mut parts: Vec<Part> = Vec::new();
    for id in row.file_ids() {
        let (path, member): (String, String) = stmt.query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let path = PathBuf::from(path);
        if games.is_some_and(|g| path.starts_with(g)) || !source.contains(&path) {
            continue;
        }
        let chd = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("chd"));
        if member.is_empty() && !chd {
            return Ok(Err(format!(
                "{} is a loose file; arcade ROMs are imported in their set's zip",
                path.display()
            )));
        }
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        // A CHD lives in a folder named after its set.
        let name = match (chd, path.parent().and_then(Path::file_name)) {
            (true, Some(set)) => format!("{}/{file}", set.to_string_lossy()),
            _ => file,
        };
        // Keyed by where it goes: one file per name in `Games/<System>/`.
        let to = dir.join(&name);
        if parts.iter().any(|p| p.path == path || p.name == name) || taken.contains(&to) || to.exists() {
            continue;
        }
        let size = std::fs::metadata(&path)?.len();
        parts.push(Part {
            name,
            path,
            member: String::new(),
            size,
            sha1: String::new(),
        });
    }
    if parts.is_empty() {
        return Ok(Ok(None));
    }
    taken.extend(parts.iter().map(|p| dir.join(&p.name)));
    parts.sort_by(|a, b| zip_order(&a.name, &b.name));
    let own = parts
        .iter()
        .find(|p| {
            Path::new(&p.name)
                .file_stem()
                .is_some_and(|s| s.to_string_lossy() == row.title)
        })
        .unwrap_or(&parts[0]);
    Ok(Ok(Some(Item {
        row_id: row.id.clone(),
        title: row.label.clone().unwrap_or_else(|| row.title.clone()),
        system: system.key.to_owned(),
        format: Format::Set,
        target: dir.join(&own.name),
        sidecar: None,
        parts,
        trash: Vec::new(),
        kept: Vec::new(),
        copies: Vec::new(),
        sheet: None,
        existing: false,
        folder: None,
        code: None,
    })))
}

/// Copy an arcade set's files whole, each read back and checked. A file
/// there already (since the plan) must be the same one, else the set
/// fails: its original would go to the trash with no copy of it.
pub(super) fn write_set(conn: &Connection, batch: i64, item: &Item) -> Result<(), Error> {
    let dir = item.target.parent().unwrap_or(Path::new("."));
    for p in &item.parts {
        let to = dir.join(&p.name);
        if to.symlink_metadata().is_ok() {
            if crate::organize::same_file(&p.path, &to)? {
                continue;
            }
            return Err(Error::Cleanup(format!(
                "{} is there already and differs from {}",
                to.display(),
                p.path.display()
            )));
        }
        let folder = to.parent().unwrap_or(dir);
        if !folder.exists() {
            std::fs::create_dir_all(folder)?;
            if folder != dir {
                journal_output(conn, batch, folder, "setdir")?;
            }
        }
        let name = to.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let part = folder.join(layout::part(&name));
        let _ = std::fs::remove_file(&part);
        let r = (|| -> Result<(), Error> {
            let mut w = File::create_new(&part)?;
            let sum = copy_hashed(BufReader::new(File::open(&p.path)?), &mut w)?;
            w.sync_all()?;
            if sha1_of(BufReader::new(File::open(&part)?))? != sum {
                return Err(Error::Cleanup(format!(
                    "{}: doesn't read back as written",
                    to.display()
                )));
            }
            super::write::place(&part, &to)
        })();
        if let Err(e) = r {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
        journal_output(conn, batch, &to, "file")?;
    }
    Ok(())
}
