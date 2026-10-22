use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::home::Home;
use crate::job::Cancel;
use crate::organize::{rename_noreplace, sanitize};
use crate::{Error, layout, settings, systems};

use super::write::move_path;
use super::{CleanupPlan, CleanupReport, Format, Item};

mod item;

/// What an item's turn comes to on the calling thread.
enum Turn<'a> {
    /// Written on a worker (or failed or skipped there).
    Worker(item::Prepared, Option<Result<item::Written, Error>>),
    /// Couldn't be prepared.
    Failed(String),
    /// A set, a loose game, or one already in place: done here.
    Here(&'a Item),
}

/// Carry out a plan. Each game is written next to its target, read back
/// and checked, then moved into place; originals go to the trash only after
/// that (and not at all with "keep originals"). Journaled for [`undo`](super::undo).
///
/// Games are written on the Settings `threads` (zips and CHDs at once);
/// the journal is kept in plan order on this thread. `progress(n, title)`
/// follows each game done, in order. Asked to stop, it finishes the games
/// already started, leaves the rest as they are, and still trashes the
/// originals of the finished ones (`report.stopped`).
pub fn apply(
    conn: &mut Connection,
    plan: &CleanupPlan,
    home: Option<&Home>,
    cancel: &Cancel,
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
    let threads = crate::parallel::threads(s.threads);
    let items: Vec<(usize, &Item)> = plan.items.iter().enumerate().collect();
    // Each game's details come from one view of the library, built once.
    let view = crate::view::library(conn)?;
    // A few games per thread at a time: what they need from the database
    // (art and details) is read for one chunk only.
    for chunk in items.chunks(threads * 2) {
        if cancel.is_set() {
            report.stopped = true;
            break;
        }
        let turns: Vec<(usize, &Item, Turn)> = chunk
            .iter()
            .map(|&(n, item)| {
                let turn = if !item::on_worker(item) {
                    Turn::Here(item)
                } else {
                    let prepared = item::prepare(conn, &view, item, staging_root.join(n.to_string()))
                        .and_then(|p| item::journal_intent(conn, batch_id, item).map(|()| p));
                    match prepared {
                        Ok(p) => Turn::Worker(p, None),
                        Err(e) => Turn::Failed(e.to_string()),
                    }
                };
                (n, item, turn)
            })
            .collect();
        let mut stopped = false;
        crate::parallel::map_ordered(
            turns,
            threads,
            |(n, item, turn)| match turn {
                Turn::Worker(p, None) if !cancel.is_set() => {
                    let w = item::write(item, &p);
                    (n, item, Turn::Worker(p, Some(w)))
                }
                t => (n, item, t),
            },
            |_, (n, item, turn)| {
                let result: Option<Result<bool, Error>> = match turn {
                    Turn::Here(i) if i.existing => {
                        // Nothing to write; its copies go with the originals
                        // below, once it is checked still there.
                        let staging = staging_root.join(n.to_string());
                        match item::still_held(conn, i, &plan.dest, &staging) {
                            Ok(()) => ok.push(i),
                            Err(e) => report.failed.push((i.title.clone(), e.to_string())),
                        }
                        let _ = std::fs::remove_dir_all(&staging);
                        progress(n, &i.title);
                        return;
                    }
                    Turn::Here(i) => {
                        let staging = staging_root.join(n.to_string());
                        let r = if cancel.is_set() {
                            None
                        } else {
                            Some(item::write_here(conn, &view, batch_id, i, &staging))
                        };
                        let _ = std::fs::remove_dir_all(&staging);
                        r
                    }
                    Turn::Failed(e) => Some(Err(Error::Cleanup(e))),
                    Turn::Worker(p, w) => {
                        let _ = std::fs::remove_dir_all(p.staging());
                        match w {
                            Some(Ok(w)) => {
                                Some(item::journal(conn, batch_id, item, &w).map(|()| w.took_sheet))
                            }
                            // Not written: its intent rows go.
                            w => match item::forget_intent(conn, batch_id, item) {
                                Err(e) => Some(Err(e)),
                                Ok(()) => w.map(|w| w.map(|w| w.took_sheet)),
                            },
                        }
                    }
                };
                match result {
                    // Not started: asked to stop.
                    None => stopped = true,
                    Some(Ok(took_sheet)) => {
                        report.done += 1;
                        ok.push(item);
                        // A cue sheet that didn't fit the tracks keeps its file.
                        if let (false, Some(sheet)) = (took_sheet, &item.sheet) {
                            failed_paths.insert(sheet.path.clone());
                        }
                    }
                    Some(Err(e)) => {
                        report.failed.push((item.title.clone(), e.to_string()));
                        failed_paths.extend(item.parts.iter().map(|p| p.path.clone()));
                    }
                }
                progress(n, &item.title);
            },
        );
        if stopped {
            report.stopped = true;
            break;
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
                match trash(conn, batch_id, original, home, &batch_id.to_string()) {
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
        match rename_noreplace(&tmp, &list) {
            Ok(()) => {}
            // One appeared meanwhile: it stays.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = std::fs::remove_file(&tmp);
                continue;
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e.into());
            }
        }
        journal_output(conn, batch, &list, "file")?;
        n += 1;
    }
    Ok(n)
}

/// What is in a folder this batch wrote (the folder itself is journaled
/// already, before it): the folders first (so undo, last first, empties
/// them before removing them), then each file. Undo removes just these;
/// anything put there since stays.
fn journal_tree(conn: &Connection, batch: i64, dir: &Path) -> Result<(), Error> {
    let mut files = Vec::new();
    for e in walkdir::WalkDir::new(dir).min_depth(1).sort_by_file_name() {
        let e = e.map_err(std::io::Error::from)?;
        if e.file_type().is_dir() {
            journal_output(conn, batch, e.path(), "setdir")?;
        } else {
            files.push(e.into_path());
        }
    }
    for f in files {
        journal_output(conn, batch, &f, "file")?;
    }
    Ok(())
}

pub(crate) fn journal_output(conn: &Connection, batch: i64, path: &Path, kind: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO cleanup_outputs (batch_id, path, kind) VALUES (?1, ?2, ?3)",
        params![batch, path.to_string_lossy(), kind],
    )?;
    Ok(())
}

/// `RomForge/.romforge-trash/<folder>/<the original's full path>`, or
/// without a RomForge folder `<folder of original>/.romforge-trash/<folder>/<name>`.
/// `folder` is the batch number (originals), or `undo-<batch>` (what an
/// undo took away).
pub(super) fn trash_path(original: &Path, home: Option<&Home>, folder: &str) -> PathBuf {
    match home {
        Some(h) => {
            let rel: PathBuf = original
                .components()
                .filter(|c| matches!(c, std::path::Component::Normal(_)))
                .collect();
            h.trash().join(folder).join(rel)
        }
        None => original
            .parent()
            .unwrap_or(Path::new("."))
            .join(layout::TRASH_DIR)
            .join(folder)
            .join(original.file_name().unwrap_or_default()),
    }
}

/// Move `original` to [`trash_path`], journaled under `batch` (so emptying
/// the trash finds it, and undo can put an original back).
pub(super) fn trash(
    conn: &Connection,
    batch: i64,
    original: &Path,
    home: Option<&Home>,
    folder: &str,
) -> Result<(), Error> {
    trash_to(conn, batch, original, &trash_path(original, home, folder))
}

/// Move `original` to `to` in the trash, journaled under `batch`.
pub(super) fn trash_to(conn: &Connection, batch: i64, original: &Path, to: &Path) -> Result<(), Error> {
    // Journaled before it moves: a crash in between leaves a row undo can
    // act on, never a trashed file nothing knows about.
    conn.execute(
        "INSERT INTO cleanup_trash (batch_id, original, trashed) VALUES (?1, ?2, ?3)",
        params![batch, original.to_string_lossy(), to.to_string_lossy()],
    )?;
    if let Err(e) = move_path(original, to) {
        // Something is in the trash already (a move cut short after the
        // copy was in place): the row stays, so undo can put it back.
        if to.symlink_metadata().is_ok() {
            return Err(e.into());
        }
        conn.execute(
            "DELETE FROM cleanup_trash WHERE batch_id = ?1 AND trashed = ?2",
            params![batch, to.to_string_lossy()],
        )?;
        return Err(e.into());
    }
    Ok(())
}
