//! romforge core: DAT import, scanning, hashing and identification.
//!
//! Milestone 1 is read-only with respect to ROM files: nothing here moves,
//! renames, writes or extracts them.

pub mod dat;
mod db;
pub mod fetch;
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
    #[error("download: {0}")]
    Http(String),
    #[error("no DAT source is known for {0}")]
    NoDownload(&'static str),
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

/// How an imported DAT compares with the latest one upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatState {
    Current,
    Outdated,
    Missing,
    /// Imported from libretro's No-Intro mirror; update with `dat download`.
    Mirror,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DatStatus {
    pub name: String,
    pub system: &'static str,
    pub latest: String,
    pub dat_id: Option<i64>,
    pub imported: Option<String>,
    pub state: DatState,
}

fn read_file(path: &Path) -> Result<Vec<u8>, Error> {
    let mut buf = Vec::new();
    BufReader::new(std::fs::File::open(path)?).read_to_end(&mut buf)?;
    Ok(buf)
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

    /// Import a DAT (Logiqx XML or clrmamepro text). The system is taken from `system_key` when
    /// given, else guessed from the DAT name; if neither works the DAT has no
    /// system (it still matches when a folder is scanned without a scope).
    pub fn import_dat(
        &mut self,
        path: impl AsRef<Path>,
        system_key: Option<&str>,
    ) -> Result<ImportedDat, Error> {
        let path = std::path::absolute(path.as_ref())?;
        let bytes = read_file(&path)?;
        self.import_dat_bytes(&bytes, &path.to_string_lossy(), system_key)
    }

    /// Import a DAT file, or every DAT inside a `.zip` of DATs (as No-Intro
    /// and Redump hand them out). `system_key` is refused for a zip holding
    /// more than one DAT, since they are for different systems.
    pub fn import_dats(
        &mut self,
        path: impl AsRef<Path>,
        system_key: Option<&str>,
    ) -> Result<Vec<ImportedDat>, Error> {
        let path = std::path::absolute(path.as_ref())?;
        let dats = fetch::dats_in(&read_file(&path)?)?;
        if system_key.is_some() && dats.len() > 1 {
            return Err(Error::Dat(format!(
                "{} holds {} DATs; --system applies to one",
                path.display(),
                dats.len()
            )));
        }
        dats.into_iter()
            .map(|fetch::DatBytes { member, bytes }| {
                let source = match member {
                    Some(m) => format!("{}#{m}", path.display()),
                    None => path.to_string_lossy().into_owned(),
                };
                self.import_dat_bytes(&bytes, &source, system_key)
            })
            .collect()
    }

    /// Download a source's DATs and import them for `system_key`: one DAT,
    /// or every DAT of a TOSEC system. Downloading the same DAT again changes
    /// nothing; a newer one replaces the old. An empty result means the
    /// TOSEC release is already imported for this system, so the pack was not
    /// fetched.
    pub fn download_dat(
        &mut self,
        system_key: &str,
        source: fetch::Source,
    ) -> Result<Vec<ImportedDat>, Error> {
        if let fetch::Source::Tosec(name) = source {
            let url = fetch::tosec_pack_url()?;
            let prefix = format!("{url}#TOSEC/{name} - ");
            let have: bool = self.conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM dats WHERE substr(source_path, 1, length(?1)) = ?1)",
                [&prefix],
                |r| r.get(0),
            )?;
            if have {
                return Ok(Vec::new());
            }
            let pack = fetch::tosec_pack(&url)?;
            let dats = fetch::tosec_dats(&pack, name)?;
            if dats.is_empty() {
                return Err(Error::Dat(format!("the TOSEC pack has no {name} DATs")));
            }
            return dats
                .into_iter()
                .map(|d| {
                    let source = format!("{url}#{}", d.member.unwrap_or_default());
                    self.import_dat_bytes(&d.bytes, &source, Some(system_key))
                })
                .collect();
        }
        let url = source.url().unwrap_or_default();
        let body = fetch::get(&url)?;
        let head = String::from_utf8_lossy(&body[..body.len().min(512)]).to_ascii_lowercase();
        if head.contains("<html") || head.contains("<!doctype html") {
            return Err(Error::Http(format!("{url}: got a web page, not a DAT")));
        }
        let mut dats = fetch::dats_in(&body)?;
        if dats.len() != 1 {
            return Err(Error::Dat(format!(
                "{url}: expected one DAT, found {}",
                dats.len()
            )));
        }
        let dat = dats.remove(0);
        Ok(vec![self.import_dat_bytes(&dat.bytes, &url, Some(system_key))?])
    }

    /// Download and import every DAT of a system ([`fetch::sources`]; hacks
    /// and homebrew with `extras`), reporting each source as it finishes;
    /// one failing does not stop the rest.
    pub fn download_dats(
        &mut self,
        system_key: &str,
        extras: bool,
        mut each: impl FnMut(fetch::Source, Result<Vec<ImportedDat>, Error>),
    ) -> Result<(), Error> {
        let system =
            systems::by_key(system_key).ok_or_else(|| Error::UnknownSystem(system_key.to_owned()))?;
        let sources = fetch::sources(system, extras);
        if sources.is_empty() {
            return Err(Error::NoDownload(system.key));
        }
        for source in sources {
            let result = self.download_dat(system.key, source);
            each(source, result);
        }
        Ok(())
    }

    /// Compare imported DATs with No-Intro's `profile.xml` (from
    /// [`fetch::nointro_profile`]). Lists every profile entry for a known
    /// system. A system's main DAT counts as [`DatState::Mirror`] when it
    /// came from libretro's mirror, whose names and version format differ.
    pub fn nointro_status(&self, profile: &[fetch::ProfileEntry]) -> Result<Vec<DatStatus>, Error> {
        let mut by_name = self
            .conn
            .prepare("SELECT dat_id, version, source_path FROM dats WHERE name = ?1")?;
        let mut mirror_of = self.conn.prepare(
            "SELECT dat_id, version, source_path FROM dats WHERE system_id = ?1 AND source_path LIKE ?2
             ORDER BY dat_id LIMIT 1",
        )?;
        let mirror_like = format!("{}%", fetch::LIBRETRO_NOINTRO_BASE);
        let mut out = Vec::new();
        for entry in profile {
            let Some(system) = systems::from_dat_name(&entry.name) else {
                continue;
            };
            let row = |r: &rusqlite::Row| -> rusqlite::Result<(i64, Option<String>, String)> {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            };
            let mut imported = by_name.query_row([&entry.name], row).optional()?;
            let main_set = !entry.name.ends_with(')');
            if imported.is_none() && main_set {
                imported = mirror_of
                    .query_row(params![system.id, mirror_like], row)
                    .optional()?;
            }
            let state = match &imported {
                None => DatState::Missing,
                Some((_, _, src)) if fetch::Source::is_mirror_url(src) => DatState::Mirror,
                Some((_, v, _)) if v.as_deref() == Some(entry.version.as_str()) => DatState::Current,
                Some(_) => DatState::Outdated,
            };
            out.push(DatStatus {
                name: entry.name.clone(),
                system: system.key,
                latest: entry.version.clone(),
                dat_id: imported.as_ref().map(|i| i.0),
                imported: imported.and_then(|i| i.1),
                state,
            });
        }
        Ok(out)
    }

    /// Import one DAT from memory. `source` is recorded as where it came
    /// from (a path, `zip#member` or URL).
    pub fn import_dat_bytes(
        &mut self,
        bytes: &[u8],
        source: &str,
        system_key: Option<&str>,
    ) -> Result<ImportedDat, Error> {
        let sha256: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
        let dat = dat::parse_any(bytes)?;

        let system = match system_key {
            Some(key) => Some(systems::by_key(key).ok_or_else(|| Error::UnknownSystem(key.to_owned()))?),
            None => systems::from_dat_name(&dat.name),
        };

        let tx = self.conn.transaction()?;
        // The same file again is a no-op; a new version of a DAT (same name,
        // and same source family: see `fetch::source_family`) replaces the
        // old one, and folders scoped to it follow along. No-Intro and Redump both name a DAT
        // "Sony - PlayStation Portable", so a download never replaces a
        // same-named DAT from elsewhere.
        let existing: Option<(i64, String)> = {
            let mut stmt = tx.prepare(
                "SELECT dat_id, source_sha256, source_path FROM dats
                 WHERE source_sha256 = ?2 OR name = ?1
                 ORDER BY source_sha256 = ?2 DESC, dat_id",
            )?;
            let rows = stmt.query_map(params![dat.name, sha256], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            let family = fetch::source_family(source);
            let mut found = None;
            for row in rows {
                let (id, sha, path) = row?;
                if sha == sha256 || fetch::source_family(&path) == family {
                    found = Some((id, sha));
                    break;
                }
            }
            found
        };
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
                source,
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
