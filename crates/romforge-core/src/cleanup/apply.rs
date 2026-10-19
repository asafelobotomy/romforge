use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::home::Home;
use crate::organize::sanitize;
use crate::{Error, layout, settings, systems};

use super::chd::write_chd;
use super::items::write_set;
use super::parts::extras;
use super::write::{copy_folder, move_path, write_folder_zip, write_loose, write_zip};
use super::{CleanupPlan, CleanupReport, Format, Item};

/// Carry out a plan. Each game is written next to its target, read back
/// and checked, then moved into place; originals go to the trash only after
/// that (and not at all with "keep originals"). Journaled for [`undo`].
pub fn apply(
    conn: &mut Connection,
    plan: &CleanupPlan,
    home: Option<&Home>,
    mut progress: impl FnMut(usize, &str),
) -> Result<CleanupReport, Error> {
    let s = settings::load(conn)?;
    conn.execute(
        concat!(
            "INSERT INTO cleanup_batches (created_at, keep_originals) VALUES (",
            crate::db::now!(),
            ", ?1)"
        ),
        [plan.keep_originals],
    )?;
    let batch_id = conn.last_insert_rowid();
    let mut report = CleanupReport {
        batch_id,
        ..CleanupReport::default()
    };
    let staging_root = home
        .map_or_else(|| s.staging(), Home::staging)
        .join(format!("batch-{batch_id}"));
    let mut ok: Vec<&Item> = Vec::new();
    let mut failed_paths: BTreeSet<PathBuf> = BTreeSet::new();
    for (n, item) in plan.items.iter().enumerate() {
        progress(n, &item.title);
        if item.existing {
            // Nothing to write; its copies go with the originals below.
            ok.push(item);
            continue;
        }
        let staging = staging_root.join(n.to_string());
        let result = (|| -> Result<bool, Error> {
            if item.target.exists() {
                return Err(Error::Cleanup(format!(
                    "{} already exists",
                    item.target.display()
                )));
            }
            let dir = item.target.parent().unwrap_or(Path::new("."));
            let new_dir = !dir.exists();
            std::fs::create_dir_all(dir)?;
            if item.format == Format::Set {
                if new_dir {
                    journal_output(conn, batch_id, dir, "setdir")?;
                }
                write_set(conn, batch_id, item)?;
                return Ok(false);
            }
            if item.format == Format::Loose {
                // The set's folder is journaled before what goes in it, so
                // undo (last first) empties it before removing it.
                if new_dir {
                    journal_output(conn, batch_id, dir, "setdir")?;
                }
                let written = extras(conn, &item.row_id)
                    .and_then(|extras| write_loose(conn, batch_id, item, &extras, &staging));
                if let Err(e) = written {
                    if new_dir && std::fs::remove_dir(dir).is_ok() {
                        conn.execute(
                            "DELETE FROM cleanup_outputs WHERE batch_id = ?1 AND path = ?2",
                            params![batch_id, dir.to_string_lossy()],
                        )?;
                    }
                    return Err(e);
                }
                return Ok(false);
            }
            if item.format == Format::Dir {
                let name = item.target.file_name().unwrap_or_default().to_string_lossy();
                let part = dir.join(layout::part(&name));
                let _ = std::fs::remove_dir_all(&part);
                let extras = extras(conn, &item.row_id)?;
                if let Err(e) = copy_folder(item, &extras, &part) {
                    let _ = std::fs::remove_dir_all(&part);
                    return Err(e);
                }
                if item.target.exists() {
                    let _ = std::fs::remove_dir_all(&part);
                    return Err(Error::Cleanup(format!(
                        "{} already exists",
                        item.target.display()
                    )));
                }
                std::fs::rename(&part, &item.target)?;
                journal_output(conn, batch_id, &item.target, "dir")?;
                return Ok(false);
            }
            let name = item
                .target
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let part = dir.join(layout::part(&name));
            let _ = std::fs::remove_file(&part);
            let extras = extras(conn, &item.row_id)?;
            let written = match item.format {
                Format::Zip => write_zip(item, &extras, &staging, &part).map(|()| false),
                Format::Folder => write_folder_zip(item, &extras, &staging, &part).map(|()| false),
                _ => write_chd(item, &staging, &part),
            };
            let took_sheet = match written {
                Ok(t) => t,
                Err(e) => {
                    let _ = std::fs::remove_file(&part);
                    return Err(e);
                }
            };
            // The sidecar is written to a temporary folder and renamed too.
            if let Some(side) = &item.sidecar {
                let tmp = dir.join(layout::side(&name));
                let _ = std::fs::remove_dir_all(&tmp);
                std::fs::create_dir_all(&tmp)?;
                for (f, data) in &extras {
                    std::fs::write(tmp.join(f), data)?;
                }
                if side.exists() {
                    let _ = std::fs::remove_dir_all(&tmp);
                    let _ = std::fs::remove_file(&part);
                    return Err(Error::Cleanup(format!("{} already exists", side.display())));
                }
                std::fs::rename(&tmp, side)?;
                journal_output(conn, batch_id, side, "dir")?;
            }
            if item.target.exists() {
                let _ = std::fs::remove_file(&part);
                return Err(Error::Cleanup(format!(
                    "{} already exists",
                    item.target.display()
                )));
            }
            std::fs::rename(&part, &item.target)?;
            journal_output(conn, batch_id, &item.target, "file")?;
            Ok(took_sheet)
        })();
        let _ = std::fs::remove_dir_all(&staging);
        match result {
            Ok(took_sheet) => {
                report.done += 1;
                ok.push(item);
                // A cue sheet that didn't fit the tracks keeps its file.
                if let (false, Some(sheet)) = (took_sheet, &item.sheet) {
                    failed_paths.insert(sheet.path.clone());
                }
            }
            Err(e) => {
                report.failed.push((item.title.clone(), e.to_string()));
                failed_paths.extend(item.parts.iter().map(|p| p.path.clone()));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&staging_root);
    let written: Vec<&Item> = ok.iter().copied().filter(|i| !i.existing).collect();
    match write_playlists(conn, batch_id, &written) {
        Ok(n) => report.playlists = n,
        Err(e) => report.failed.push(("playlists".into(), e.to_string())),
    }
    // Originals last, once every game in them is safely written.
    if !plan.keep_originals {
        for item in ok {
            for original in item.trash.iter().filter(|p| !failed_paths.contains(*p)) {
                match trash(conn, batch_id, original, home) {
                    Ok(()) => report.trashed += 1,
                    Err(e) => report
                        .failed
                        .push((item.title.clone(), format!("{}: {e}", original.display()))),
                }
            }
        }
    }
    // A batch that left nothing behind (no file, no journal row) is
    // forgotten.
    if report.done == 0 && report.trashed == 0 {
        let gone = conn.execute(
            "DELETE FROM cleanup_batches WHERE batch_id = ?1
               AND NOT EXISTS (SELECT 1 FROM cleanup_outputs WHERE batch_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM cleanup_trash WHERE batch_id = ?1)",
            [batch_id],
        )?;
        if gone > 0 {
            report.batch_id = 0;
        }
    }
    Ok(report)
}

/// A title's disk or disc number, the set size if it says ("Disk 1 of 4",
/// TOSEC; "Disc 2", Redump), and the title without that group.
pub fn disk_of(title: &str) -> Option<(u32, Option<u32>, String)> {
    for word in ["(Disk ", "(Disc "] {
        let Some(at) = title.find(word) else { continue };
        let rest = &title[at + word.len()..];
        let end = rest.find(')')?;
        let inner = &rest[..end];
        let (n, of) = match inner.split_once(" of ") {
            Some((n, m)) => (n.trim().parse().ok()?, Some(m.trim().parse().ok()?)),
            None => (inner.trim().parse().ok()?, None),
        };
        let base = format!("{}{}", &title[..at], &rest[end + 1..]);
        let base = base.split_whitespace().collect::<Vec<_>>().join(" ");
        return Some((n, of, base.trim().to_owned()));
    }
    None
}

/// For each multi-disk set this batch wrote a disk of, a playlist in the
/// same folder listing its disks in order, once every disk is there (all
/// "of N", else disks 1, 2, ... with no gap): `<set>.m3u` of the zips or
/// CHDs; for a set kept loose, the playlist its core reads (`.m3u` of the
/// images, or np2kai's `.cmd`, one command line), or none. An existing
/// playlist is left alone. Journaled, so undo removes it.
pub(super) fn write_playlists(conn: &Connection, batch: i64, written: &[&Item]) -> Result<usize, Error> {
    // (folder, set, playlist kind, loose)
    let mut sets: BTreeSet<(PathBuf, String, &'static str, bool)> = BTreeSet::new();
    for it in written {
        let Some((_, _, base)) = disk_of(&it.title) else {
            continue;
        };
        let Some(dir) = it.target.parent() else { continue };
        let kind = match it.format {
            Format::Loose => systems::by_key(&it.system)
                .and_then(|s| s.loose_sets)
                .unwrap_or(""),
            _ => "m3u",
        };
        if !kind.is_empty() {
            sets.insert((dir.to_owned(), base, kind, it.format == Format::Loose));
        }
    }
    let mut n = 0;
    for (dir, base, kind, loose) in sets {
        let list = dir.join(format!("{}.{kind}", sanitize(&base)));
        if list.exists() {
            continue;
        }
        // Every disk of the set in the folder, by disk number.
        let mut disks: BTreeMap<u32, (Option<u32>, String)> = BTreeMap::new();
        for e in std::fs::read_dir(&dir)?.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some((stem, ext)) = name.rsplit_once('.') else {
                continue;
            };
            let ext = ext.to_ascii_lowercase();
            let wanted = if loose {
                !name.starts_with('.') && !["m3u", "cmd"].contains(&ext.as_str())
            } else {
                ["zip", "chd", "7z"].contains(&ext.as_str())
            };
            if !wanted || !e.path().is_file() {
                continue;
            }
            if let Some((d, of, b)) = disk_of(stem)
                && sanitize(&b) == sanitize(&base)
            {
                disks.entry(d).or_insert((of, name));
            }
        }
        let count = disks.values().find_map(|(of, _)| *of);
        let complete = disks.len() >= 2
            && disks.keys().copied().eq(1..=disks.len() as u32)
            && count.is_none_or(|c| c as usize == disks.len());
        if !complete {
            continue;
        }
        let body: String = if kind == "cmd" {
            // np2kai reads one line of at most 1023 bytes: the program,
            // then the disks (quoted, relative to the file).
            let line: String = std::iter::once("np2kai".to_owned())
                .chain(disks.values().map(|(_, f)| format!("\"{f}\"")))
                .collect::<Vec<_>>()
                .join(" ");
            if line.len() > 1023 || disks.values().any(|(_, f)| f.contains('"')) {
                continue;
            }
            line + "\n"
        } else {
            disks.values().map(|(_, f)| format!("{f}\n")).collect()
        };
        let tmp = dir.join(layout::part(&sanitize(&base)));
        std::fs::write(&tmp, body)?;
        if list.exists() {
            let _ = std::fs::remove_file(&tmp);
            continue;
        }
        std::fs::rename(&tmp, &list)?;
        journal_output(conn, batch, &list, "file")?;
        n += 1;
    }
    Ok(n)
}

pub(crate) fn journal_output(conn: &Connection, batch: i64, path: &Path, kind: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO cleanup_outputs (batch_id, path, kind) VALUES (?1, ?2, ?3)",
        params![batch, path.to_string_lossy(), kind],
    )?;
    Ok(())
}

/// `RomForge/.romforge-trash/<batch>/<the original's full path>`, or
/// without a RomForge folder `<folder of original>/.romforge-trash/<batch>/<name>`.
pub(super) fn trash(
    conn: &Connection,
    batch: i64,
    original: &Path,
    home: Option<&Home>,
) -> Result<(), Error> {
    let to = match home {
        Some(h) => {
            let rel: PathBuf = original
                .components()
                .filter(|c| matches!(c, std::path::Component::Normal(_)))
                .collect();
            h.trash().join(batch.to_string()).join(rel)
        }
        None => original
            .parent()
            .unwrap_or(Path::new("."))
            .join(layout::TRASH_DIR)
            .join(batch.to_string())
            .join(original.file_name().unwrap_or_default()),
    };
    // Journaled before it moves: a crash in between leaves a row undo can
    // act on, never a trashed file nothing knows about.
    conn.execute(
        "INSERT INTO cleanup_trash (batch_id, original, trashed) VALUES (?1, ?2, ?3)",
        params![batch, original.to_string_lossy(), to.to_string_lossy()],
    )?;
    if let Err(e) = move_path(original, &to) {
        conn.execute(
            "DELETE FROM cleanup_trash WHERE batch_id = ?1 AND trashed = ?2",
            params![batch, to.to_string_lossy()],
        )?;
        return Err(e.into());
    }
    Ok(())
}
