//! Scanning a folder.

mod store;
mod visits;

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::scan::Visit;
use crate::{
    Error, Library, ScanOptions, ScanProgress, ScanReport, Scope, SkippedKind, identify, layout, patcher,
    scan, scummvm, systems,
};

/// Every arcade set name (lower case) of the imported DATs: an archive
/// named after one has all its members hashed.
fn arcade_sets(conn: &Connection) -> Result<std::collections::HashSet<String>, Error> {
    let ids: Vec<i64> = systems::SYSTEMS
        .iter()
        .filter(|s| s.arcade)
        .map(|s| s.id)
        .collect();
    let mut out = std::collections::HashSet::new();
    let mut stmt = conn.prepare(
        "SELECT DISTINCT lower(g.name) FROM games g JOIN dats d ON d.dat_id = g.dat_id WHERE d.system_id = ?1",
    )?;
    for id in ids {
        for n in stmt.query_map([id], |r| r.get::<_, String>(0))? {
            out.insert(n?);
        }
    }
    Ok(out)
}

impl Library {
    /// [`Library::scan_with`] with default options and no progress.
    pub fn scan(&mut self, root: impl AsRef<Path>, scope: Scope) -> Result<ScanReport, Error> {
        self.scan_with(root, scope, &ScanOptions::default(), |_| {})
    }

    /// Hash the likely ROMs under `root` (all files with
    /// [`ScanOptions::all_files`]) and identify them against `scope`.
    /// Rescanning a folder replaces its previous results.
    pub fn scan_with(
        &mut self,
        root: impl AsRef<Path>,
        scope: Scope,
        opts: &ScanOptions,
        mut progress: impl FnMut(ScanProgress),
    ) -> Result<ScanReport, Error> {
        let root = std::fs::canonicalize(root.as_ref())
            .map_err(|e| std::io::Error::new(e.kind(), format!("{}: {e}", root.as_ref().display())))?;
        let (system_id, dat_id) = match scope {
            Scope::All => (None, None),
            Scope::System(id) => {
                systems::by_id(id).ok_or_else(|| Error::UnknownSystem(id.to_string()))?;
                (Some(id), None)
            }
            Scope::Dat(id) => (None, Some(identify::resolve_dat(&self.conn, id)?)),
        };
        let skip: Vec<PathBuf> = self
            .path
            .iter()
            .flat_map(|p| {
                ["", "-wal", "-shm", "-journal"].map(|s| PathBuf::from(format!("{}{s}", p.display())))
            })
            .collect();

        // No thread count given: the Settings one (itself 0 for automatic).
        let opts = &ScanOptions {
            jobs: match opts.jobs {
                0 => crate::settings::load(&self.conn)?.threads,
                n => n,
            },
            ..opts.clone()
        };
        let scummvm_list = self.extdb_dir().map(|d| d.join(layout::SCUMMVM_LIST));
        let arcade_sets = arcade_sets(&self.conn)?;
        // Paths are keys: one that isn't UTF-8 can't be stored as itself.
        let root_str = root
            .to_str()
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{}: path is not valid UTF-8", root.display()),
                )
            })?
            .to_owned();
        // Immediate: this reads, then writes; a deferred transaction could
        // fail on the write if another connection wrote in between.
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        // What is already known under the folder (from any scan made with the
        // same choice of files: `all_files` hashes members a plain scan
        // skips): files whose size and modification time haven't changed
        // keep their rows.
        let prefix = format!("{}/", root_str.trim_end_matches('/'));
        let known: std::collections::HashMap<String, scan::Stamp> = if opts.rehash {
            Default::default()
        } else {
            tx.prepare(
                "SELECT DISTINCT f.path, f.disk_size, f.mtime FROM files f
                 JOIN scan_roots s ON s.root_id = f.root_id
                 WHERE (f.path = ?1 OR substr(f.path, 1, length(?2)) = ?2)
                   AND f.disk_size IS NOT NULL AND f.mtime IS NOT NULL
                   AND s.all_files = ?3",
            )?
            .query_map(params![root_str, prefix, opts.all_files], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    scan::Stamp {
                        size: r.get(1)?,
                        mtime: r.get(2)?,
                    },
                ))
            })?
            .collect::<Result<_, _>>()?
        };
        let root_id: i64 = tx.query_row(
            concat!(
                "INSERT INTO scan_roots (path, system_id, dat_id, scanned_at, all_files)
             VALUES (?1, ?2, ?3, ",
                crate::db::now!(),
                ", ?4)
             ON CONFLICT(path) DO UPDATE SET system_id = excluded.system_id,
               dat_id = excluded.dat_id, scanned_at = excluded.scanned_at,
               all_files = excluded.all_files
             RETURNING root_id"
            ),
            params![root_str, system_id, dat_id, opts.all_files],
            |r| r.get(0),
        )?;
        // Folders scanned on their own inside this one, deepest first: a
        // file belongs to the deepest scanned folder holding it, so each
        // keeps its scope when the outer folder is rescanned.
        let mut nested: Vec<(PathBuf, i64)> = tx
            .prepare("SELECT path, root_id FROM scan_roots WHERE substr(path, 1, length(?1)) = ?1")?
            .query_map([&prefix], |r| {
                Ok((PathBuf::from(r.get::<_, String>(0)?), r.get(1)?))
            })?
            .collect::<Result<_, _>>()?;
        nested.sort_by_key(|(p, _)| std::cmp::Reverse(p.as_os_str().len()));
        let owner = |p: &Path| {
            nested
                .iter()
                .find(|(r, _)| p.starts_with(r))
                .map_or(root_id, |(_, id)| *id)
        };
        // Paths seen in this scan; rows of this folder for anything else are
        // files that went away.
        let mut seen: std::collections::HashSet<String> = Default::default();
        // Folders the walk couldn't read: what is known under them is kept
        // (seen), as an unreadable file keeps its row.
        let mut unreadable_dirs: Vec<String> = Vec::new();

        let mut report = ScanReport {
            root: root.clone(),
            ..ScanReport::default()
        };
        let mut skipped_kinds: std::collections::BTreeMap<String, SkippedKind> = Default::default();
        // Patch files (or zips holding one) seen, to pair with their games.
        let mut patch_files: std::collections::BTreeSet<PathBuf> = Default::default();
        // ScummVM games are named by their files' sizes and first bytes:
        // found first, so those files aren't hashed one by one.
        let games = scummvm::games_under(&tx, scummvm_list.as_deref(), &root)?;
        let hashed_ids = {
            let mut store = store::Store::new(&tx)?;
            let mut adopt = tx.prepare("UPDATE files SET root_id = ?1 WHERE path = ?2")?;
            let mut result = Ok(());
            let mut units = ScanProgress::default();
            let unchanged =
                |path: &Path, stamp: scan::Stamp| path.to_str().is_some_and(|p| known.get(p) == Some(&stamp));
            let walked = scan::walk(&root, &skip, &games, opts, &arcade_sets, unchanged, |visit| {
                if result.is_err() {
                    return;
                }
                match visit {
                    Visit::Hashed(entry) => {
                        // One that can't be stored (a path that isn't
                        // UTF-8) is an error, not a hashed file.
                        if let (Ok(h), Some(_)) = (&entry.hashes, entry.path.to_str()) {
                            report.hashed += 1;
                            report.bytes_hashed += h.raw.size;
                        }
                        store.hold(entry);
                        (units.hashed, units.bytes_hashed, units.skipped) =
                            (report.hashed, report.bytes_hashed, report.skipped);
                    }
                    Visit::Planned { units: n, bytes } => {
                        (units.units_total, units.disk_total) = (n, bytes);
                    }
                    Visit::UnitDone { path, bytes } => {
                        units.units_done += 1;
                        units.disk_done += bytes;
                        result =
                            store.done(&path, owner(&path), &mut report, &mut seen, &mut unreadable_dirs);
                    }
                    Visit::Skipped { path, member } => {
                        visits::skipped(&mut report, &mut skipped_kinds, &mut patch_files, path, member);
                        return;
                    }
                    Visit::InGame { files, .. } => {
                        report.in_games += files;
                        return;
                    }
                    Visit::SkippedLink(path) => {
                        report.skipped_links.push(path);
                        return;
                    }
                    Visit::Unchanged(path) => {
                        // Kept, but scanned as part of this folder now (and
                        // so with its scope), as a re-hashed file would be.
                        let Some(p) = path.to_str().map(str::to_owned) else {
                            return;
                        };
                        if let Err(e) = adopt.execute(params![owner(&path), p]) {
                            result = Err(e);
                        }
                        seen.insert(p);
                        report.unchanged += 1;
                        return;
                    }
                }
                progress(units);
            });
            result?;
            // Stopped (or the walk failed): keep the files finished since
            // the last commit before saying so.
            if walked.is_err() {
                store.commit(&report)?;
            }
            walked?;
            for path in store.left() {
                store.done(&path, owner(&path), &mut report, &mut seen, &mut unreadable_dirs)?;
            }
            store.unstamp(&report)?;
            std::mem::take(&mut store.hashed_ids)
        };
        // Rows of this folder (and the folders inside it, walked too) for
        // files that are gone (or now skipped).
        let ids: std::collections::BTreeSet<i64> = std::iter::once(root_id)
            .chain(nested.iter().map(|(_, id)| *id))
            .collect();
        for id in ids {
            let ours: Vec<String> = tx
                .prepare("SELECT DISTINCT path FROM files WHERE root_id = ?1")?
                .query_map([id], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            let kept =
                |p: &str| seen.contains(p) || unreadable_dirs.iter().any(|d| p.starts_with(d.as_str()));
            for path in ours.iter().filter(|p| !kept(p)) {
                tx.execute(
                    "DELETE FROM files WHERE path = ?1 AND root_id = ?2",
                    params![path, id],
                )?;
            }
        }
        identify::identify(&tx, identify::Relook::Files(&hashed_ids))?;
        patcher::record(&tx, root_id, &patch_files, &seen)?;
        patcher::refresh(&tx)?;
        scummvm::refresh(&tx, scummvm_list.as_deref())?;
        tx.commit()?;
        let kinds: Vec<SkippedKind> = skipped_kinds.into_values().collect();
        report.skipped_kinds = SkippedKind::merge([kinds.as_slice()]);
        Ok(report)
    }
}

#[cfg(test)]
mod tests;
