//! romforge core: DAT import, scanning, hashing and identification.
//!
//! Milestone 1 is read-only with respect to ROM files: nothing here moves,
//! renames, writes or extracts them.

pub mod dat;
mod db;
pub mod hash;
pub mod headers;
mod identify;
mod report;
pub mod scan;
pub mod systems;

use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest as _, Sha256};

pub use identify::{Scope, Status};
pub use report::{DatGame, DatSummary, LibraryFile, LibraryFilter, Summary, SystemInfo};
pub use scan::ScanOptions;
use scan::Visit;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("database: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("7z: {0}")]
    SevenZ(#[from] sevenz_rust2::Error),
    #[error("DAT: {0}")]
    Dat(String),
    #[error("database schema version {0} is newer than this romforge")]
    Schema(i64),
    #[error("unknown system {0:?}")]
    UnknownSystem(String),
    #[error("no DAT with id {0}")]
    UnknownDat(i64),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ImportedDat {
    pub dat_id: i64,
    pub name: String,
    pub system: Option<&'static str>,
    pub header_hint: Option<String>,
    pub games: usize,
    pub roms: usize,
    /// The older DAT with the same name that this import replaced.
    pub replaced: Option<i64>,
    /// The exact same file was already imported; nothing changed.
    pub unchanged: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ScanReport {
    pub root: PathBuf,
    pub hashed: usize,
    pub bytes_hashed: u64,
    /// Files and archive members without a ROM extension (not hashed).
    pub skipped: usize,
    /// Symlinked folders inside the tree (not followed).
    pub skipped_links: Vec<PathBuf>,
    pub errors: Vec<(PathBuf, String, String)>,
}

/// Running totals passed to the progress callback after every file or member.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct ScanProgress {
    pub hashed: usize,
    pub bytes_hashed: u64,
    pub skipped: usize,
}

/// The default database location: `$XDG_DATA_HOME/romforge/romforge.db`.
pub fn default_db_path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("romforge").join("romforge.db")
}

pub struct Library {
    conn: Connection,
    path: Option<PathBuf>,
}

impl Library {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let conn = db::open(path)?;
        Ok(Self {
            conn,
            path: Some(std::path::absolute(path)?),
        })
    }

    pub fn open_in_memory() -> Result<Self, Error> {
        Ok(Self {
            conn: db::open_in_memory()?,
            path: None,
        })
    }

    /// Import a Logiqx XML DAT. The system is taken from `system_key` when
    /// given, else guessed from the DAT name; if neither works the DAT has no
    /// system (it still matches when a folder is scanned without a scope).
    pub fn import_dat(
        &mut self,
        path: impl AsRef<Path>,
        system_key: Option<&str>,
    ) -> Result<ImportedDat, Error> {
        let path = std::path::absolute(path.as_ref())?;
        let bytes = {
            let mut buf = Vec::new();
            BufReader::new(std::fs::File::open(&path)?).read_to_end(&mut buf)?;
            buf
        };
        let sha256: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let dat = dat::parse(&bytes[..])?;

        let system = match system_key {
            Some(key) => Some(systems::by_key(key).ok_or_else(|| Error::UnknownSystem(key.to_owned()))?),
            None => systems::from_dat_name(&dat.name),
        };

        let tx = self.conn.transaction()?;
        // The same file again is a no-op; a new version of a DAT (same name)
        // replaces the old one, and folders scoped to it follow along.
        let existing: Option<(i64, String)> = tx
            .query_row(
                "SELECT dat_id, source_sha256 FROM dats WHERE name = ?1 OR source_sha256 = ?2
                 ORDER BY source_sha256 = ?2 DESC LIMIT 1",
                params![dat.name, sha256],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((old_id, old_sha)) = &existing
            && *old_sha == sha256
        {
            return Ok(ImportedDat {
                dat_id: *old_id,
                name: dat.name,
                system: system.map(|s| s.key),
                header_hint: dat.header_hint,
                games: dat.games.len(),
                roms: dat.games.iter().map(|g| g.roms.len()).sum(),
                replaced: None,
                unchanged: true,
            });
        }
        tx.execute(
            "INSERT INTO dats (system_id, name, description, version, author, homepage, url,
                               header_hint, source_path, source_sha256, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
            params![
                system.map(|s| s.id),
                dat.name,
                dat.description,
                dat.version,
                dat.author,
                dat.homepage,
                dat.url,
                dat.header_hint,
                path.to_string_lossy(),
                sha256,
            ],
        )?;
        let dat_id = tx.last_insert_rowid();
        let replaced = existing.map(|(id, _)| id);
        if let Some(old_id) = replaced {
            tx.execute(
                "UPDATE scan_roots SET dat_id = ?1 WHERE dat_id = ?2",
                [dat_id, old_id],
            )?;
            tx.execute("DELETE FROM dats WHERE dat_id = ?1", [old_id])?;
        }
        let mut roms = 0;
        {
            let mut add_game =
                tx.prepare("INSERT INTO games (dat_id, name, description) VALUES (?1, ?2, ?3)")?;
            let mut add_rom = tx.prepare(
                "INSERT INTO roms (game_id, name, size, crc32, md5, sha1, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for game in &dat.games {
                add_game.execute(params![dat_id, game.name, game.description])?;
                let game_id = tx.last_insert_rowid();
                for rom in &game.roms {
                    add_rom.execute(params![
                        game_id, rom.name, rom.size, rom.crc32, rom.md5, rom.sha1, rom.status
                    ])?;
                    roms += 1;
                }
            }
        }
        identify::identify_all(&tx)?;
        tx.commit()?;

        Ok(ImportedDat {
            dat_id,
            name: dat.name,
            system: system.map(|s| s.key),
            header_hint: dat.header_hint,
            games: dat.games.len(),
            roms,
            replaced,
            unchanged: false,
        })
    }

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
        let root = std::fs::canonicalize(root.as_ref())?;
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

        let tx = self.conn.transaction()?;
        let root_str = root.to_string_lossy().into_owned();
        let root_id: i64 = tx.query_row(
            "INSERT INTO scan_roots (path, system_id, dat_id, scanned_at)
             VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
             ON CONFLICT(path) DO UPDATE SET system_id = excluded.system_id,
               dat_id = excluded.dat_id, scanned_at = excluded.scanned_at
             RETURNING root_id",
            params![root_str, system_id, dat_id],
            |r| r.get(0),
        )?;
        tx.execute("DELETE FROM files WHERE root_id = ?1", [root_id])?;

        let mut report = ScanReport {
            root: root.clone(),
            ..ScanReport::default()
        };
        {
            let mut upsert = tx.prepare(
                "INSERT INTO files (root_id, path, member, size, crc32, md5, sha1, header_kind,
                                    stripped_size, stripped_crc32, stripped_md5, stripped_sha1, scanned_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                 ON CONFLICT(path, member) DO UPDATE SET root_id = excluded.root_id, size = excluded.size,
                   crc32 = excluded.crc32, md5 = excluded.md5, sha1 = excluded.sha1,
                   header_kind = excluded.header_kind, stripped_size = excluded.stripped_size,
                   stripped_crc32 = excluded.stripped_crc32, stripped_md5 = excluded.stripped_md5,
                   stripped_sha1 = excluded.stripped_sha1, scanned_at = excluded.scanned_at",
            )?;
            let mut result = Ok(());
            scan::walk(&root, &skip, opts, |visit| {
                if result.is_err() {
                    return;
                }
                let entry = match visit {
                    Visit::Hashed(entry) => entry,
                    Visit::Skipped { .. } => {
                        report.skipped += 1;
                        return;
                    }
                    Visit::SkippedLink(path) => {
                        report.skipped_links.push(path);
                        return;
                    }
                };
                match entry.hashes {
                    Ok(h) => {
                        let s = h.stripped.as_ref();
                        result = upsert
                            .execute(params![
                                root_id,
                                entry.path.to_string_lossy(),
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
                            ])
                            .map(drop);
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
        identify::identify_all(&tx)?;
        tx.commit()?;
        Ok(report)
    }

    /// Recompute all matches, e.g. after removing a DAT.
    pub fn identify(&mut self) -> Result<(), Error> {
        let tx = self.conn.transaction()?;
        identify::identify_all(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn summary(&self) -> Result<Summary, Error> {
        report::summary(&self.conn)
    }

    pub fn dat_games(&self, dat_id: i64) -> Result<Vec<DatGame>, Error> {
        identify::resolve_dat(&self.conn, dat_id)?;
        report::dat_games(&self.conn, dat_id)
    }

    pub fn files(&self, filter: &LibraryFilter) -> Result<Vec<LibraryFile>, Error> {
        report::files(&self.conn, filter)
    }

    pub fn systems_in_use(&self) -> Result<Vec<SystemInfo>, Error> {
        report::systems_in_use(&self.conn)
    }
}
