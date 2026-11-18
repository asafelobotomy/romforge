//! Writing what a scan hashed: a file's rows go in together once its read
//! is done (an archive's members all at once), and what is written is
//! committed every couple of seconds. A scan stopped or failed halfway
//! keeps the files it finished (stamped, so the next scan doesn't read them
//! again, and `pending` until they are looked up in the DATs); a file it
//! was still reading has no new rows, so it is read again next time.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::{Connection, Statement, params};

use crate::scan::Entry;
use crate::{ScanReport, disc};

/// How long written rows wait for a commit at most (in this crate's tests,
/// no time: every file is committed).
#[cfg(not(test))]
const COMMIT_EVERY: Duration = Duration::from_secs(2);
#[cfg(test)]
const COMMIT_EVERY: Duration = Duration::ZERO;

pub(super) struct Store<'t> {
    tx: &'t Connection,
    upsert: Statement<'t>,
    drop_path: Statement<'t>,
    set_chd: Statement<'t>,
    unstamp: Statement<'t>,
    chd_sha1s: HashMap<String, Option<String>>,
    /// Paths whose old rows were dropped before the new ones went in (an
    /// archive may have lost members).
    cleared: HashSet<String>,
    /// Files hashed in this scan: only they are looked up in the DATs again.
    pub hashed_ids: HashSet<i64>,
    /// What came for each file still being read.
    open: HashMap<PathBuf, Vec<Entry>>,
    /// When the last commit was.
    since: Instant,
    /// How many of the report's errors have had their stamps taken off.
    unstamped: usize,
}

impl<'t> Store<'t> {
    pub fn new(tx: &'t Connection) -> rusqlite::Result<Self> {
        Ok(Self {
            tx,
            upsert: tx.prepare(concat!(
                "INSERT INTO files (root_id, path, member, size, crc32, md5, sha1, header_kind,
                                    stripped_size, stripped_crc32, stripped_md5, stripped_sha1, scanned_at,
                                    disk_size, mtime, id_kind, id_serial, id_title, id_region, sheet_files,
                                    id_size, id_crc, pending)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ",
                crate::db::now!(),
                ",
                         ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, 1)
                 ON CONFLICT(path, member) DO UPDATE SET root_id = excluded.root_id, size = excluded.size,
                   crc32 = excluded.crc32, md5 = excluded.md5, sha1 = excluded.sha1,
                   header_kind = excluded.header_kind, stripped_size = excluded.stripped_size,
                   stripped_crc32 = excluded.stripped_crc32, stripped_md5 = excluded.stripped_md5,
                   stripped_sha1 = excluded.stripped_sha1, scanned_at = excluded.scanned_at,
                   disk_size = excluded.disk_size, mtime = excluded.mtime,
                   id_kind = excluded.id_kind, id_serial = excluded.id_serial,
                   id_title = excluded.id_title, id_region = excluded.id_region,
                   sheet_files = excluded.sheet_files, id_size = excluded.id_size, id_crc = excluded.id_crc,
                   pending = 1
                 RETURNING file_id"
            ))?,
            drop_path: tx.prepare("DELETE FROM files WHERE path = ?1")?,
            set_chd: tx.prepare("UPDATE files SET chd_sha1 = ?2 WHERE file_id = ?1")?,
            unstamp: tx.prepare("UPDATE files SET mtime = NULL WHERE path = ?1")?,
            chd_sha1s: HashMap::new(),
            cleared: HashSet::new(),
            hashed_ids: HashSet::new(),
            open: HashMap::new(),
            since: Instant::now(),
            unstamped: 0,
        })
    }

    /// Something hashed (or failed) in a file still being read.
    pub fn hold(&mut self, entry: Entry) {
        self.open.entry(entry.path.clone()).or_default().push(entry);
    }

    /// What came outside any file's read (a folder the walk couldn't
    /// open): the paths still held once the walk is over.
    pub fn left(&self) -> Vec<PathBuf> {
        self.open.keys().cloned().collect()
    }

    /// `path` is read: write what came for it, and commit if it is time.
    pub fn done(
        &mut self,
        path: &Path,
        owner: i64,
        report: &mut ScanReport,
        seen: &mut HashSet<String>,
        unreadable_dirs: &mut Vec<String>,
    ) -> rusqlite::Result<()> {
        for entry in self.open.remove(path).unwrap_or_default() {
            self.put(entry, owner, report, seen, unreadable_dirs)?;
        }
        if self.since.elapsed() >= COMMIT_EVERY {
            self.commit(report)?;
        }
        Ok(())
    }

    /// Commit what is written (the scan's transaction goes on). Also when
    /// a scan stops: the files it finished since the last commit stay.
    pub fn commit(&mut self, report: &ScanReport) -> rusqlite::Result<()> {
        self.unstamp(report)?;
        self.tx.execute_batch("COMMIT; BEGIN IMMEDIATE")?;
        self.since = Instant::now();
        Ok(())
    }

    /// A path with an error (an unreadable file, an archive member that
    /// failed) isn't stamped, so the next scan reads it again instead of
    /// calling it unchanged.
    pub fn unstamp(&mut self, report: &ScanReport) -> rusqlite::Result<()> {
        for (path, _, _) in &report.errors[self.unstamped..] {
            if let Some(p) = path.to_str() {
                self.unstamp.execute([p])?;
            }
        }
        self.unstamped = report.errors.len();
        Ok(())
    }

    fn put(
        &mut self,
        entry: Entry,
        owner: i64,
        report: &mut ScanReport,
        seen: &mut HashSet<String>,
        unreadable_dirs: &mut Vec<String>,
    ) -> rusqlite::Result<()> {
        let Some(path_str) = entry.path.to_str().map(str::to_owned) else {
            // Not stored under a mangled name: reported instead.
            report.errors.push((
                entry.path,
                entry.member,
                "path is not valid UTF-8; not stored".into(),
            ));
            return Ok(());
        };
        seen.insert(path_str.clone());
        // A plain file that couldn't be read keeps its old row (a flaky
        // disk shouldn't forget it); it is read again next time.
        let unreadable = entry.member.is_empty() && entry.hashes.is_err();
        if unreadable && !path_str.is_empty() {
            unreadable_dirs.push(format!("{}/", path_str.trim_end_matches('/')));
        }
        if !unreadable && self.cleared.insert(path_str.clone()) {
            self.drop_path.execute([&path_str])?;
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
        let h = match entry.hashes {
            Ok(h) => h,
            Err(e) => {
                report.errors.push((entry.path, entry.member, e));
                return Ok(());
            }
        };
        let s = h.stripped.as_ref();
        let id = self.upsert.query_row(
            params![
                owner,
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
        )?;
        self.hashed_ids.insert(id);
        // A CHD's own SHA1, which arcade DATs give for a disk.
        let is_chd = entry
            .path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("chd"));
        if is_chd {
            let sha1 = self
                .chd_sha1s
                .entry(path_str)
                .or_insert_with(|| disc::chd_sha1(&entry.path))
                .clone();
            self.set_chd.execute(params![id, sha1])?;
        }
        Ok(())
    }
}
