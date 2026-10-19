//! Scanning a folder.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::scan::Visit;
use crate::{
    Error, Library, ScanOptions, ScanProgress, ScanReport, Scope, SkippedKind, disc, identify, layout, patch,
    patcher, scan, scummvm, systems,
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
        self.scan_with(root, scope, ScanOptions::default(), |_| {})
    }

    /// Hash the likely ROMs under `root` (all files with
    /// [`ScanOptions::all_files`]) and identify them against `scope`.
    /// Rescanning a folder replaces its previous results.
    pub fn scan_with(
        &mut self,
        root: impl AsRef<Path>,
        scope: Scope,
        opts: ScanOptions,
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
        // files that went away. `cleared`: paths whose old rows were dropped
        // before the new ones went in (an archive may have lost members).
        let mut seen: std::collections::HashSet<String> = Default::default();
        // Files hashed in this scan: only they are looked up in the DATs again.
        let mut hashed_ids: std::collections::HashSet<i64> = Default::default();
        let mut cleared: std::collections::HashSet<String> = Default::default();

        let mut report = ScanReport {
            root: root.clone(),
            ..ScanReport::default()
        };
        let mut skipped_kinds: std::collections::BTreeMap<String, SkippedKind> = Default::default();
        // Patch files (or zips holding one) seen, to pair with their games.
        let mut patch_files: std::collections::BTreeSet<PathBuf> = Default::default();
        {
            let mut upsert = tx.prepare(
                concat!("INSERT INTO files (root_id, path, member, size, crc32, md5, sha1, header_kind,
                                    stripped_size, stripped_crc32, stripped_md5, stripped_sha1, scanned_at,
                                    disk_size, mtime, id_kind, id_serial, id_title, id_region, sheet_files, id_size, id_crc)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ", crate::db::now!(), ",
                         ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
                 ON CONFLICT(path, member) DO UPDATE SET root_id = excluded.root_id, size = excluded.size,
                   crc32 = excluded.crc32, md5 = excluded.md5, sha1 = excluded.sha1,
                   header_kind = excluded.header_kind, stripped_size = excluded.stripped_size,
                   stripped_crc32 = excluded.stripped_crc32, stripped_md5 = excluded.stripped_md5,
                   stripped_sha1 = excluded.stripped_sha1, scanned_at = excluded.scanned_at,
                   disk_size = excluded.disk_size, mtime = excluded.mtime,
                   id_kind = excluded.id_kind, id_serial = excluded.id_serial,
                   id_title = excluded.id_title, id_region = excluded.id_region,
                   sheet_files = excluded.sheet_files, id_size = excluded.id_size, id_crc = excluded.id_crc
                 RETURNING file_id"),
            )?;
            let mut drop_path = tx.prepare("DELETE FROM files WHERE path = ?1")?;
            let mut set_chd = tx.prepare("UPDATE files SET chd_sha1 = ?2 WHERE file_id = ?1")?;
            let mut chd_sha1s: std::collections::HashMap<String, Option<String>> = Default::default();
            let mut adopt = tx.prepare("UPDATE files SET root_id = ?1 WHERE path = ?2")?;
            let mut result = Ok(());
            let unchanged =
                |path: &Path, stamp: scan::Stamp| path.to_str().is_some_and(|p| known.get(p) == Some(&stamp));
            scan::walk(&root, &skip, opts, &arcade_sets, unchanged, |visit| {
                if result.is_err() {
                    return;
                }
                let entry = match visit {
                    Visit::Hashed(entry) => entry,
                    Visit::Skipped { path, member } => {
                        report.skipped += 1;
                        let name = if member.is_empty() {
                            path.file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default()
                        } else {
                            member.clone()
                        };
                        if patch::is_patch_name(&name) && path.to_str().is_some() {
                            patch_files.insert(path.clone());
                        }
                        if !layout::is_extra(&member) && !layout::is_extra(&name) {
                            let ext = std::path::Path::new(&name)
                                .extension()
                                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                                .unwrap_or_default();
                            skipped_kinds
                                .entry(ext.clone())
                                .and_modify(|k: &mut SkippedKind| k.count += 1)
                                .or_insert_with(|| SkippedKind {
                                    ext,
                                    count: 1,
                                    example: if member.is_empty() {
                                        path.display().to_string()
                                    } else {
                                        format!("{}#{member}", path.display())
                                    },
                                });
                        }
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
                };
                let Some(path_str) = entry.path.to_str().map(str::to_owned) else {
                    // Not stored under a mangled name: reported instead.
                    report.errors.push((
                        entry.path,
                        entry.member,
                        "path is not valid UTF-8; not stored".into(),
                    ));
                    return;
                };
                seen.insert(path_str.clone());
                // A plain file that couldn't be read keeps its old row (a
                // flaky disk shouldn't forget it); it is read again next time.
                let unreadable = entry.member.is_empty() && entry.hashes.is_err();
                if !unreadable
                    && cleared.insert(path_str.clone())
                    && let Err(e) = drop_path.execute([&path_str])
                {
                    result = Err(e);
                    return;
                }
                if entry.member.is_empty()
                    && let Some(listed) = disc::sheet_files(&entry.path)
                {
                    match listed {
                        Ok(files) => {
                            for f in files.iter().filter(|f| !f.exists()) {
                                report
                                    .sheet_problems
                                    .push((entry.path.clone(), format!("missing {}", f.display())));
                            }
                        }
                        Err(e) => report.sheet_problems.push((entry.path.clone(), e.to_string())),
                    }
                }
                match entry.hashes {
                    Ok(h) => {
                        let s = h.stripped.as_ref();
                        result = upsert
                            .query_row(
                                params![
                                    owner(&entry.path),
                                    path_str,
                                    entry.member,
                                    h.raw.size,
                                    h.raw.crc32,
                                    h.raw.md5,
                                    h.raw.sha1,
                                    s.map(|s| s.0),
                                    s.map(|s| s.1.size),
                                    s.map(|s| &s.1.crc32),
                                    s.map(|s| &s.1.md5),
                                    s.map(|s| &s.1.sha1),
                                    entry.stamp.map(|s| s.size),
                                    entry.stamp.map(|s| s.mtime),
                                    h.id.as_ref().map(|i| i.kind),
                                    h.id.as_ref().and_then(|i| i.serial.as_deref()),
                                    h.id.as_ref().and_then(|i| i.title.as_deref()),
                                    h.id.as_ref().and_then(|i| i.region.as_deref()),
                                    h.sheet.as_ref().and_then(|s| serde_json::to_string(s).ok()),
                                    h.id.as_ref().and_then(|i| i.image_size),
                                    h.id.as_ref().and_then(|i| i.source_crc),
                                ],
                                |r| r.get::<_, i64>(0),
                            )
                            .inspect(|&id| {
                                hashed_ids.insert(id);
                            })
                            .and_then(|id| {
                                // A CHD's own SHA1, which arcade DATs give
                                // for a disk.
                                let is_chd = entry
                                    .path
                                    .extension()
                                    .is_some_and(|e| e.eq_ignore_ascii_case("chd"));
                                if !is_chd {
                                    return Ok(());
                                }
                                let sha1 = chd_sha1s
                                    .entry(path_str.clone())
                                    .or_insert_with(|| disc::chd_sha1(&entry.path))
                                    .clone();
                                set_chd.execute(params![id, sha1]).map(|_| ())
                            });
                        report.hashed += 1;
                        report.bytes_hashed += h.raw.size;
                    }
                    Err(e) => report.errors.push((entry.path, entry.member, e)),
                }
                progress(ScanProgress {
                    hashed: report.hashed,
                    bytes_hashed: report.bytes_hashed,
                    skipped: report.skipped,
                });
            })?;
            result?;
        }
        // A path with an error (an unreadable file, an archive member that
        // failed) isn't stamped, so the next scan reads it again instead of
        // calling it unchanged.
        for path in report.errors.iter().filter_map(|(p, _, _)| p.to_str()) {
            tx.execute("UPDATE files SET mtime = NULL WHERE path = ?1", [path])?;
        }
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
            for path in ours.iter().filter(|p| !seen.contains(*p)) {
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
