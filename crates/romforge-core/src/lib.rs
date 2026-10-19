//! romforge core: DAT import, scanning, hashing and identification.
//!
//! Scanning and identifying never touch ROM files. Only [`organize`]
//! (milestone 2) moves and renames them, and only [`cleanup`] (milestone
//! 10) repacks them, each after a dry run and journaled for undo.

pub mod artsources;
pub mod cleanup;
pub mod dat;
mod db;
pub mod disc;
pub mod enrich;
pub mod extdb;
pub mod fetch;
pub mod hash;
pub mod headers;
pub mod home;
pub mod ident;
mod identify;
pub mod launch;
pub mod layout;
pub mod names;
pub mod organize;
pub mod patch;
pub mod patcher;
mod report;
pub mod scan;
pub mod scummvm;
pub mod search;
pub mod settings;
pub mod suggest;
pub mod systems;
pub mod titles;
mod vcdiff;
pub mod view;
pub mod wikidata;

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
    #[error("organize: {0}")]
    Organize(String),
    #[error("CHD: {0}")]
    Chd(String),
    #[error("launch: {0}")]
    Launch(String),
    #[error("settings: {0}")]
    Settings(String),
    #[error("search: {0}")]
    Search(String),
    #[error("cleanup: {0}")]
    Cleanup(String),
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
    /// What those were, by extension, most first, so a format romforge
    /// doesn't know shows here. Left out: details files (`.romforge.md`)
    /// and the `romforge/` folder inside archives.
    pub skipped_kinds: Vec<SkippedKind>,
    /// Symlinked folders inside the tree (not followed).
    pub skipped_links: Vec<PathBuf>,
    /// Files (or archives) unchanged since the last scan, not read again.
    pub unchanged: usize,
    pub errors: Vec<(PathBuf, String, String)>,
    /// Loose `.cue`/`.gdi` sheets and the track files they name that are
    /// missing.
    pub sheet_problems: Vec<(PathBuf, String)>,
}

/// Files a scan skipped for their extension, by extension.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SkippedKind {
    /// Lower case, without the dot; empty for none.
    pub ext: String,
    pub count: usize,
    /// One of them (`path` or `path#member`).
    pub example: String,
}

impl SkippedKind {
    /// Several scans' kinds as one list, most first.
    pub fn merge<'a>(lists: impl IntoIterator<Item = &'a [SkippedKind]>) -> Vec<SkippedKind> {
        let mut by: std::collections::BTreeMap<String, SkippedKind> = Default::default();
        for k in lists.into_iter().flatten() {
            by.entry(k.ext.clone())
                .and_modify(|e| e.count += k.count)
                .or_insert_with(|| k.clone());
        }
        let mut v: Vec<SkippedKind> = by.into_values().collect();
        v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.ext.cmp(&b.ext)));
        v
    }

    /// `.txt 5, .nfo 3, no extension 2, and 4 more kinds`, at most `n`.
    pub fn summary(kinds: &[SkippedKind], n: usize) -> String {
        let mut parts: Vec<String> = kinds
            .iter()
            .take(n)
            .map(|k| {
                if k.ext.is_empty() {
                    format!("no extension {}", k.count)
                } else {
                    format!(".{} {}", k.ext, k.count)
                }
            })
            .collect();
        if kinds.len() > n {
            parts.push(format!("and {} more kinds", kinds.len() - n));
        }
        parts.join(", ")
    }
}

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

/// One source folder's rescan: its report, or the folder and why it failed.
pub type RefreshResult = Result<ScanReport, (PathBuf, String)>;

/// What [`Library::scummvm_import`] kept: entries in the list, ScummVM
/// games found in the scanned folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ScummvmList {
    pub entries: usize,
    pub games: usize,
    /// Folders ScummVM's --detect named (including ones its list does).
    pub asked: usize,
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

/// `s` for a `LIKE ... ESCAPE '\'` pattern: `%`, `_` and `\` match
/// themselves.
pub(crate) fn like_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn read_file(path: &Path) -> Result<Vec<u8>, Error> {
    let mut buf = Vec::new();
    BufReader::new(std::fs::File::open(path)?).read_to_end(&mut buf)?;
    Ok(buf)
}

/// The default database location: `$XDG_DATA_HOME/romforge/romforge.db`.
pub fn default_db_path() -> PathBuf {
    layout::xdg("XDG_DATA_HOME", ".local/share")
        .unwrap_or_else(|| PathBuf::from(layout::APP_DIR))
        .join(layout::DB_FILE)
}

pub struct Library {
    conn: Connection,
    path: Option<PathBuf>,
    /// The RomForge folder the database is in (milestone 13), if it is.
    home: Option<home::Home>,
    /// Inside [`Library::batch`]: DAT imports skip re-identifying.
    batch_depth: u32,
    /// A DAT was imported inside the batch; identify when it ends.
    batch_dirty: bool,
    /// The library view, kept until the database changes (by any
    /// connection: SQLite's `data_version`, and this one's own changes).
    view_cache: std::sync::Mutex<Option<CachedView>>,
}

/// ((SQLite data_version, this connection's total changes), the view).
type CachedView = ((i64, u64), std::sync::Arc<view::LibraryView>);

impl Library {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let mut conn = db::open(path)?;
        let home = home::Home::of_db(path);
        if let Some(h) = &home {
            home::heal(&mut conn, h.root())?;
        }
        Ok(Self {
            conn,
            home,
            path: Some(std::path::absolute(path)?),
            batch_depth: 0,
            batch_dirty: false,
            view_cache: Default::default(),
        })
    }

    pub fn open_in_memory() -> Result<Self, Error> {
        Ok(Self {
            conn: db::open_in_memory()?,
            path: None,
            home: None,
            batch_depth: 0,
            batch_dirty: false,
            view_cache: Default::default(),
        })
    }

    /// Run `f` with DAT imports deferring the re-identification of scanned
    /// files, then identify once at the end (also when `f` fails, since the
    /// DATs imported before the failure stay). Every import otherwise
    /// re-checks the whole library, which adds up over hundreds of DATs.
    /// Batches nest; only the outermost one identifies.
    pub fn batch<T, E: From<Error>>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, E>) -> Result<T, E> {
        self.batch_depth += 1;
        let result = f(self);
        self.batch_depth -= 1;
        if self.batch_depth == 0 {
            // A TOSEC pack is shared within one batch only.
            fetch::forget_tosec_pack();
            if std::mem::take(&mut self.batch_dirty) {
                self.identify()?;
            }
        }
        result
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
        self.batch(|lib| {
            dats.into_iter()
                .map(|fetch::DatBytes { member, bytes }| {
                    let source = match member {
                        Some(m) => format!("{}#{m}", path.display()),
                        None => path.to_string_lossy().into_owned(),
                    };
                    lib.import_dat_bytes(&bytes, &source, system_key)
                })
                .collect()
        })
    }

    /// Store a libretro serial list (`fetch::Source::Serials`) for
    /// `system_key`: each record under its own title, and under the name of
    /// every DAT game of the system with one of its CRC32s (No-Intro may
    /// have renamed it since). Returns how many serials were stored. A
    /// system libretro has no list for stores none.
    pub fn download_serials(&mut self, system_key: &str, source: fetch::Source) -> Result<usize, Error> {
        let Some(url) = source.url() else {
            return Ok(0);
        };
        let Some(body) = fetch::get_optional(&url)? else {
            return Ok(0);
        };
        self.store_serials(system_key, &body)
    }

    /// DAT titles of `system_key` with `serial` (compared by
    /// [`dat::serial_key`]). Several discs or versions can share a serial,
    /// so this narrows, never identifies (house rule 8).
    pub fn titles_by_serial(&self, system_key: &str, serial: &str) -> Result<Vec<String>, Error> {
        let system =
            systems::by_key(system_key).ok_or_else(|| Error::UnknownSystem(system_key.to_owned()))?;
        Ok(self
            .conn
            .prepare("SELECT DISTINCT title FROM serials WHERE system_id = ?1 AND key = ?2 ORDER BY title")?
            .query_map(params![system.id, dat::serial_key(serial)], |r| r.get(0))?
            .collect::<Result<_, _>>()?)
    }

    /// Store a Retool clone list (JSON) named `list` for `system_key`,
    /// replacing what an earlier version of that list stored. A title
    /// already in another group (of this list or another) keeps that one.
    /// Returns how many titles were stored.
    pub fn store_clone_list(&mut self, system_key: &str, list: &str, json: &[u8]) -> Result<usize, Error> {
        #[derive(serde::Deserialize)]
        struct List {
            variants: Vec<Variant>,
        }
        #[derive(serde::Deserialize)]
        struct Variant {
            group: String,
            /// None in a compilation-only entry (a multi-game cartridge),
            /// which isn't one game under other names.
            #[serde(default)]
            titles: Vec<Title>,
        }
        #[derive(serde::Deserialize)]
        struct Title {
            #[serde(rename = "searchTerm")]
            search_term: String,
            #[serde(default)]
            priority: Option<i64>,
            /// Regex and other special matching: skipped.
            #[serde(default, rename = "nameType")]
            name_type: Option<String>,
        }
        let system =
            systems::by_key(system_key).ok_or_else(|| Error::UnknownSystem(system_key.to_owned()))?;
        let parsed: List =
            serde_json::from_slice(json).map_err(|e| Error::Dat(format!("clone list: {e}")))?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM clone_groups WHERE system_id = ?1 AND list = ?2",
            params![system.id, list],
        )?;
        let mut n = 0;
        {
            let mut put = tx.prepare(
                "INSERT OR IGNORE INTO clone_groups (system_id, title, grp, priority, list)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for v in &parsed.variants {
                for t in v.titles.iter().filter(|t| t.name_type.is_none()) {
                    n += put.execute(params![
                        system.id,
                        organize::base_title(&t.search_term),
                        v.group,
                        t.priority.unwrap_or(1),
                        list
                    ])?;
                }
            }
        }
        tx.commit()?;
        Ok(n)
    }

    /// [`Library::download_serials`] with the list already in hand.
    pub fn store_serials(&mut self, system_key: &str, list: &[u8]) -> Result<usize, Error> {
        let system =
            systems::by_key(system_key).ok_or_else(|| Error::UnknownSystem(system_key.to_owned()))?;
        let records = dat::parse_cmp_records(&String::from_utf8_lossy(list))?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM serials WHERE system_id = ?1 AND source = 'libretro'",
            [system.id],
        )?;
        let mut stored = 0;
        {
            let mut put = tx.prepare(
                "INSERT OR IGNORE INTO serials (system_id, title, serial, key, source)
                 VALUES (?1, ?2, ?3, ?4, 'libretro')",
            )?;
            let mut by_crc = tx.prepare(
                "SELECT DISTINCT g.name FROM roms r JOIN games g ON g.game_id = r.game_id
                 JOIN dats d ON d.dat_id = g.dat_id WHERE d.system_id = ?1 AND r.crc32 = ?2",
            )?;
            for rec in &records {
                let serials: Vec<String> = rec
                    .fields
                    .iter()
                    .filter(|(k, _)| k == "serial")
                    .flat_map(|(_, v)| dat::split_serials(v))
                    .chain(rec.serials.iter().flat_map(|v| dat::split_serials(v)))
                    .collect();
                let mut titles: Vec<String> = rec.title.iter().cloned().collect();
                for crc in &rec.crcs {
                    for name in by_crc.query_map(params![system.id, crc], |r| r.get::<_, String>(0))? {
                        titles.push(name?);
                    }
                }
                titles.sort();
                titles.dedup();
                for title in &titles {
                    for serial in &serials {
                        stored += put.execute(params![system.id, title, serial, dat::serial_key(serial)])?;
                    }
                }
            }
        }
        tx.commit()?;
        Ok(stored)
    }

    /// Download a source's DATs and import them for `system_key`: one DAT,
    /// or every DAT of a TOSEC system. Downloading the same DAT again changes
    /// nothing; a newer one replaces the old. An empty result means the
    /// TOSEC release is already imported for this system, so the pack was not
    /// fetched (or, for `Serials`, that the list was stored).
    pub fn download_dat(
        &mut self,
        system_key: &str,
        source: fetch::Source,
    ) -> Result<Vec<ImportedDat>, Error> {
        if let fetch::Source::Serials(_) = source {
            self.download_serials(system_key, source)?;
            return Ok(Vec::new());
        }
        if let fetch::Source::CloneList(..) = source {
            if let (Some(url), fetch::Source::CloneList(name, group)) = (source.url(), source)
                && let Some(body) = fetch::get_optional(&url)?
            {
                self.store_clone_list(system_key, &format!("{name} ({group})"), &body)?;
            }
            return Ok(Vec::new());
        }
        if let fetch::Source::Tosec(name) = source {
            let url = fetch::tosec_pack_url()?;
            let (prefix, part) = tosec_paths(&url, name);
            // Done when this release is in and no older one is left (an
            // older DAT left over means the last run did not finish).
            let (have, older): (bool, bool) = self.conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM dats WHERE substr(source_path, 1, length(?1)) = ?1),
                        EXISTS (SELECT 1 FROM dats WHERE substr(source_path, 1, length(?2)) = ?2
                                  AND instr(source_path, ?3) > 0
                                  AND substr(source_path, 1, length(?1)) <> ?1)",
                params![prefix, fetch::TOSEC_BASE, part],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if have && !older {
                return Ok(Vec::new());
            }
            let pack = fetch::tosec_pack(&url)?;
            let dats = fetch::tosec_dats(&pack, name)?;
            if dats.is_empty() {
                return Err(Error::Dat(format!("the TOSEC pack has no {name} DATs")));
            }
            // Every DAT is tried even if one fails: the "already imported"
            // check above would otherwise never come back for the rest.
            return self.batch(|lib| {
                let total = dats.len();
                let mut imported = Vec::new();
                let mut failed = Vec::new();
                for d in dats {
                    let member = d.member.unwrap_or_default();
                    let source = format!("{url}#{member}");
                    match lib.import_dat_bytes(&d.bytes, &source, Some(system_key)) {
                        Ok(i) => imported.push(i),
                        Err(e) => failed.push(format!("{member}: {e}")),
                    }
                }
                if failed.is_empty() {
                    if forget_older_tosec(&lib.conn, &url, name)? > 0 {
                        lib.batch_dirty = true;
                    }
                    Ok(imported)
                } else {
                    Err(Error::Dat(format!(
                        "{} of {total} TOSEC {name} DATs failed ({} imported): {}",
                        failed.len(),
                        imported.len(),
                        failed.join("; ")
                    )))
                }
            });
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
        self.batch(|lib| {
            for source in sources {
                let result = lib.download_dat(system.key, source);
                each(source, result);
            }
            Ok(())
        })
    }

    /// [`Library::download_dats`] for every system that has a DAT source,
    /// in one batch. `each` gets the system key too.
    pub fn download_all(
        &mut self,
        extras: bool,
        mut each: impl FnMut(&'static str, fetch::Source, Result<Vec<ImportedDat>, Error>),
    ) -> Result<(), Error> {
        self.batch(|lib| {
            for s in systems::SYSTEMS
                .iter()
                .filter(|s| !fetch::sources(s, extras).is_empty())
            {
                lib.download_dats(s.key, extras, |source, r| each(s.key, source, r))?;
            }
            Ok(())
        })
    }

    /// Compare imported DATs with No-Intro's `profile.xml` (from
    /// [`fetch::nointro_profile`]). Lists every profile entry for a known
    /// system. A system's main DAT counts as [`DatState::Mirror`] when it
    /// came from libretro's mirror, whose names and version format differ.
    pub fn nointro_status(&self, profile: &[fetch::ProfileEntry]) -> Result<Vec<DatStatus>, Error> {
        let mut by_name = self.conn.prepare(
            "SELECT dat_id, version, source_path FROM dats WHERE name = ?1
                 ORDER BY source_path LIKE 'http%', dat_id DESC LIMIT 1",
        )?;
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
        let sha256 = hash::hex(&Sha256::digest(bytes));
        let mut dat = dat::parse_any(bytes)?;

        let system = match system_key {
            Some(key) => Some(systems::by_key(key).ok_or_else(|| Error::UnknownSystem(key.to_owned()))?),
            None => systems::from_dat_name(&dat.name),
        };
        // A DAT without a name (libretro's MAME XMLs) is named after its
        // system, so a newer copy replaces it.
        if dat.name.is_empty()
            && let Some(s) = system
        {
            dat.name = s.key.to_owned();
        }

        let defer = self.batch_depth > 0;
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
            concat!(
                "INSERT INTO dats (system_id, name, description, version, author, homepage, url,
                               header_hint, source_path, source_sha256, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ",
                crate::db::now!(),
                ")"
            ),
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
            let mut add_game = tx.prepare(
                "INSERT INTO games (dat_id, name, description, cloneof, region, category)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            let mut add_rom = tx.prepare(
                "INSERT INTO roms (game_id, name, size, crc32, md5, sha1, status, disk)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            let mut add_serial = tx.prepare(
                "INSERT OR IGNORE INTO serials (system_id, title, serial, key, source)
                 VALUES (?1, ?2, ?3, ?4, 'dat')",
            )?;
            // This DAT says what its games' serials are now.
            let mut clear_serials =
                tx.prepare("DELETE FROM serials WHERE system_id = ?1 AND title = ?2 AND source = 'dat'")?;
            for game in &dat.games {
                add_game.execute(params![
                    dat_id,
                    game.name,
                    game.description,
                    game.cloneof,
                    game.region,
                    game.category
                ])?;
                let game_id = tx.last_insert_rowid();
                if let Some(system) = system {
                    if replaced.is_some() {
                        clear_serials.execute(params![system.id, game.name])?;
                    }
                    for serial in &game.serials {
                        add_serial.execute(params![system.id, game.name, serial, dat::serial_key(serial)])?;
                    }
                }
                for rom in &game.roms {
                    add_rom.execute(params![
                        game_id, rom.name, rom.size, rom.crc32, rom.md5, rom.sha1, rom.status, rom.disk
                    ])?;
                    roms += 1;
                }
            }
        }
        if replaced.is_some() {
            forget_stale_serials(&tx)?;
        }
        // Archives named after one of an arcade DAT's sets were hashed
        // before by extension only: read them again, every member.
        if system.is_some_and(|s| s.arcade) {
            let names: std::collections::HashSet<String> =
                dat.games.iter().map(|g| g.name.to_lowercase()).collect();
            let archives: Vec<String> = tx
                .prepare("SELECT DISTINCT path FROM files WHERE member <> ''")?
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            let mut again = tx.prepare("UPDATE files SET mtime = NULL WHERE path = ?1")?;
            for p in archives {
                if Path::new(&p)
                    .file_stem()
                    .is_some_and(|s| names.contains(&s.to_string_lossy().to_lowercase()))
                {
                    again.execute([&p])?;
                }
            }
        }
        if defer {
            self.batch_dirty = true;
        } else {
            identify::identify_all(&tx)?;
            patcher::refresh(&tx)?;
        }
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
        let tx = self.conn.transaction()?;
        let root_str = root.to_string_lossy().into_owned();
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
                |path: &Path, stamp: scan::Stamp| known.get(path.to_string_lossy().as_ref()) == Some(&stamp);
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
                        if patch::is_patch_name(&name) {
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
                        let p = path.to_string_lossy().into_owned();
                        if let Err(e) = adopt.execute(params![root_id, p]) {
                            result = Err(e);
                        }
                        seen.insert(p);
                        report.unchanged += 1;
                        return;
                    }
                };
                let path_str = entry.path.to_string_lossy().into_owned();
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
        for (path, _, _) in &report.errors {
            tx.execute(
                "UPDATE files SET mtime = NULL WHERE path = ?1",
                [path.to_string_lossy()],
            )?;
        }
        // Rows of this folder for files that are gone (or now skipped).
        let ours: Vec<String> = tx
            .prepare("SELECT DISTINCT path FROM files WHERE root_id = ?1")?
            .query_map([root_id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for path in ours.iter().filter(|p| !seen.contains(*p)) {
            tx.execute(
                "DELETE FROM files WHERE path = ?1 AND root_id = ?2",
                params![path, root_id],
            )?;
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

    /// Keep ScummVM's detection list (as `scummvm
    /// --dump-all-detection-entries` writes it) in the databases folder and
    /// name the ScummVM games in every scanned folder by it.
    pub fn scummvm_import(&mut self, list: &[u8]) -> Result<ScummvmList, Error> {
        let entries = scummvm::parse(&String::from_utf8_lossy(list))?.len();
        let dir = self
            .extdb_dir()
            .ok_or_else(|| Error::Settings("an in-memory library keeps no ScummVM list".into()))?;
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(layout::SCUMMVM_LIST);
        let tmp = dir.join(layout::part(layout::SCUMMVM_LIST));
        std::fs::write(&tmp, list)?;
        std::fs::rename(&tmp, &path)?;
        let games = scummvm::refresh(&self.conn, Some(&path))?;
        Ok(ScummvmList {
            entries,
            games,
            asked: 0,
        })
    }

    /// Import the installed MAME's own DAT (`mame -listxml`, made on this
    /// machine, never shipped) for the Arcade system; a newer MAME's
    /// replaces it.
    pub fn mame_dat(&mut self, env: &launch::Env) -> Result<ImportedDat, Error> {
        let (xml, version) = launch::mame_listxml(env)?;
        self.import_dat_bytes(&xml, &format!("mame -listxml ({version})"), Some("Arcade"))
    }

    /// Keep libretro's `ScummVM.dat` (its names for ScummVM game ids) in
    /// the databases folder, to title ScummVM games by. Returns how many
    /// game ids it names.
    pub fn scummvm_titles(&mut self, dat: &[u8]) -> Result<usize, Error> {
        let n = scummvm::titles(&String::from_utf8_lossy(dat))?.len();
        let dir = self
            .extdb_dir()
            .ok_or_else(|| Error::Settings("an in-memory library keeps no ScummVM titles".into()))?;
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join(layout::part(layout::SCUMMVM_TITLES));
        std::fs::write(&tmp, dat)?;
        std::fs::rename(&tmp, dir.join(layout::SCUMMVM_TITLES))?;
        let list = dir.join(layout::SCUMMVM_LIST);
        scummvm::refresh(&self.conn, Some(&list))?;
        Ok(n)
    }

    /// Download libretro's `ScummVM.dat` ([`Library::scummvm_titles`]).
    pub fn scummvm_titles_download(&mut self) -> Result<usize, Error> {
        let dat = fetch::get(&format!("{}/ScummVM.dat", fetch::LIBRETRO_DAT_BASE))?;
        self.scummvm_titles(&dat)
    }

    /// Ask the installed ScummVM for its detection list, then
    /// [`Library::scummvm_import`] it; then ask it what it finds in the
    /// scanned folders (`--detect`), for the engines its list leaves out.
    pub fn scummvm_read(&mut self, env: &launch::Env) -> Result<ScummvmList, Error> {
        let list = launch::scummvm_list(env)?;
        let mut r = self.scummvm_import(&list)?;
        let mut found = Vec::new();
        for root in scummvm::top_roots(&self.conn)? {
            found.extend(scummvm::parse_detect(&launch::scummvm_detect(env, &root)?, &root));
        }
        r.asked = scummvm::remember(&self.conn, &found)?;
        let list = self.extdb_dir().map(|d| d.join(layout::SCUMMVM_LIST));
        r.games = scummvm::refresh(&self.conn, list.as_deref())?;
        Ok(r)
    }

    /// Recompute all matches, e.g. after removing a DAT.
    pub fn identify(&mut self) -> Result<(), Error> {
        let tx = self.conn.transaction()?;
        identify::identify_all(&tx)?;
        patcher::refresh(&tx)?;
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

    /// Work out where identified files would go (a dry run; nothing moves).
    /// With a RomForge folder, Import does this job (milestone 13); only
    /// undoing earlier batches is left to Organize there.
    pub fn organize_plan(&self, opts: &organize::OrganizeOptions) -> Result<organize::Plan, Error> {
        if self.home.is_some() {
            return Err(Error::Organize(
                "with a RomForge folder, Import does this (every version kept and grouped); \
                 undo still works for earlier Organize batches"
                    .into(),
            ));
        }
        organize::plan(&self.conn, opts)
    }

    /// Carry out an organize plan and journal it for [`Library::organize_undo`].
    pub fn organize_apply(&mut self, plan: &organize::Plan, dest: &Path) -> Result<organize::Applied, Error> {
        organize::apply(&mut self.conn, plan, dest)
    }

    /// Move a batch's files back (the latest batch by default).
    pub fn organize_undo(&mut self, batch_id: Option<i64>) -> Result<organize::Undone, Error> {
        organize::undo(&mut self.conn, batch_id)
    }

    pub fn organize_batches(&self) -> Result<Vec<organize::Batch>, Error> {
        organize::batches(&self.conn)
    }

    /// What cleanup & rename would do (milestone 10); nothing is touched.
    /// `only`: limit to these library row ids.
    pub fn cleanup_plan(&self, only: Option<&[String]>) -> Result<cleanup::CleanupPlan, Error> {
        cleanup::plan(&self.conn, only, self.home.as_ref(), None)
    }

    /// The import plan for the games in `from` (every folder outside the
    /// RomForge folder when `None`): where each goes in `Games/`, and what
    /// stays. Nothing is touched (milestone 13).
    pub fn import_plan(&self, from: Option<&Path>) -> Result<cleanup::CleanupPlan, Error> {
        let from = from.map(std::fs::canonicalize).transpose()?;
        cleanup::plan(&self.conn, None, self.home.as_ref(), from.as_deref())
    }

    /// Carry out a cleanup plan, then refresh so the library shows the new
    /// files.
    pub fn cleanup_apply(
        &mut self,
        plan: &cleanup::CleanupPlan,
        progress: impl FnMut(usize, &str),
    ) -> Result<cleanup::CleanupReport, Error> {
        let r = cleanup::apply(&mut self.conn, plan, self.home.as_ref(), progress)?;
        self.refresh(ScanOptions::default(), |_, _| {})?;
        Ok(r)
    }

    /// Remove a cleanup batch's new files and put its originals back (the
    /// latest batch by default), then refresh.
    pub fn cleanup_undo(&mut self, batch_id: Option<i64>) -> Result<cleanup::CleanupUndone, Error> {
        let u = cleanup::undo(&mut self.conn, batch_id)?;
        self.refresh(ScanOptions::default(), |_, _| {})?;
        Ok(u)
    }

    /// Where downloaded third-party databases live: the RomForge folder's
    /// `Databases/`, else `databases/` next to the library database; `None`
    /// for an in-memory library.
    pub fn extdb_dir(&self) -> Option<PathBuf> {
        match &self.home {
            Some(h) => Some(h.databases()),
            None => self.path.as_deref().map(extdb::dir_for),
        }
    }

    /// The RomForge folder this library lives in (milestone 13).
    pub fn home(&self) -> Option<&home::Home> {
        self.home.as_ref()
    }

    pub fn cleanup_batches(&self) -> Result<Vec<cleanup::Batch>, Error> {
        cleanup::batches(&self.conn)
    }

    /// Where a patched game goes, and the scratch space for it: the
    /// RomForge folder's `Games/`, else the Library (or Source) folder.
    fn patch_places(&self) -> Result<(PathBuf, PathBuf), Error> {
        if let Some(h) = &self.home {
            return Ok((h.games(), h.staging()));
        }
        let s = settings::load(&self.conn)?;
        let dest = s.library_dir.clone().or(s.source_dir.clone()).ok_or_else(|| {
            Error::Settings("set a Library folder (or create the RomForge folder) first".into())
        })?;
        Ok((dest, s.staging()))
    }

    /// Work out applying a patch (a file you fetched) to a game: a new
    /// copy as a translation or hack of it (milestone 19). Nothing is
    /// written.
    pub fn patch_plan(
        &self,
        row_id: &str,
        patch: &Path,
        relation: Option<view::Relation>,
        tag: Option<&str>,
    ) -> Result<patcher::PatchPlan, Error> {
        let (games, staging) = self.patch_places()?;
        std::fs::create_dir_all(&staging)?;
        patcher::plan(&self.conn, &games, &staging, row_id, patch, relation, tag)
    }

    /// Apply it: the new game is written, named, journaled (`cleanup undo`
    /// removes it) and scanned.
    pub fn patch_apply(&mut self, plan: &patcher::PatchPlan) -> Result<patcher::Patched, Error> {
        let (_, staging) = self.patch_places()?;
        let done = patcher::apply(&mut self.conn, plan, &staging)?;
        self.refresh(ScanOptions::default(), |_, _| {})?;
        // A folder outside the scanned ones (a Library not yet scanned).
        if let Some(dir) = done.target.parent()
            && !self.library_view()?.rows.iter().any(|r| r.title == done.title)
        {
            self.scan(dir, Scope::All)?;
        }
        Ok(done)
    }

    /// The one identified game whose title is `query` or contains it (any
    /// case), for commands that name a game.
    pub fn find_row(&self, query: &str) -> Result<view::GameRow, Error> {
        let rows = self.library_view()?.rows;
        if let Some(r) = rows.iter().find(|r| r.id == query || r.title == query) {
            return Ok(r.clone());
        }
        let q = query.to_lowercase();
        let found: Vec<&view::GameRow> = rows
            .iter()
            .filter(|r| r.status.identified() && r.title.to_lowercase().contains(&q))
            .collect();
        match found.as_slice() {
            [one] => Ok((*one).clone()),
            [] => Err(Error::Search(format!("no identified game like {query:?}"))),
            many => Err(Error::Search(format!(
                "{query:?} matches {} games: {}",
                many.len(),
                many.iter()
                    .take(5)
                    .map(|r| r.title.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            ))),
        }
    }

    /// The patches for a game: those a scan paired with it, then the others
    /// lying in its folders.
    pub fn patches_beside(&self, row_id: &str) -> Result<Vec<patcher::FoundPatch>, Error> {
        patcher::beside(&self.conn, row_id)
    }

    /// What is in the trash (all of it, or batches older than N days).
    pub fn trash_contents(&self, older_than_days: Option<u32>) -> Result<cleanup::TrashSummary, Error> {
        cleanup::trash_contents(&self.conn, older_than_days)
    }

    /// Delete those originals for good; their batches can't be undone after.
    pub fn empty_trash(&mut self, older_than_days: Option<u32>) -> Result<cleanup::TrashSummary, Error> {
        cleanup::empty_trash(&self.conn, older_than_days)
    }

    /// Fetch metadata and art for the games you have (milestone 3).
    pub fn enrich(
        &mut self,
        opts: &enrich::EnrichOptions,
        progress: impl FnMut(enrich::EnrichEvent),
    ) -> Result<enrich::EnrichReport, Error> {
        let opts = self.with_extdb(opts);
        enrich::enrich(&mut self.conn, &opts, progress)
    }

    /// Enrich options using this library's downloaded databases unless the
    /// caller named a folder.
    fn with_extdb(&self, opts: &enrich::EnrichOptions) -> enrich::EnrichOptions {
        let mut o = opts.clone();
        if o.extdb.is_none() {
            o.extdb = self.extdb_dir();
        }
        if o.art_dir.is_none() {
            o.art_dir = self.home.as_ref().map(home::Home::media);
        }
        o
    }

    /// Metadata and cached art for games whose name contains `query`.
    pub fn game_info(&self, query: &str) -> Result<Vec<enrich::GameInfo>, Error> {
        enrich::info(&self.conn, query)
    }

    /// Every emulator romforge could use, and whether it is installed.
    pub fn emulators(&self, env: &launch::Env) -> Result<Vec<launch::EmulatorStatus>, Error> {
        launch::status(&self.conn, env)
    }

    pub fn add_emulator(&mut self, e: &launch::UserEmulator) -> Result<(), Error> {
        launch::add_emulator(&self.conn, e)
    }

    pub fn remove_emulator(&mut self, name: &str) -> Result<usize, Error> {
        launch::remove_emulator(&self.conn, name)
    }

    /// How to start a game (by name or file path); nothing runs yet.
    pub fn launch_plan(
        &self,
        env: &launch::Env,
        query: &str,
        emulator: Option<&str>,
    ) -> Result<launch::LaunchSpec, Error> {
        launch::plan(&self.conn, env, query, emulator)
    }

    /// How to start one library row: exactly its files, never another game
    /// that shares its title.
    pub fn launch_row(
        &self,
        env: &launch::Env,
        row_id: &str,
        emulator: Option<&str>,
    ) -> Result<launch::LaunchSpec, Error> {
        launch::plan_row(&self.conn, env, row_id, emulator)
    }

    /// One row per game (unidentified files as their own rows) and the
    /// platforms they're on, for the app's library screen.
    pub fn library_view(&self) -> Result<view::LibraryView, Error> {
        Ok((*self.cached_view()?).clone())
    }

    /// Everything known about one row of [`Library::library_view`].
    pub fn game_detail(&self, id: &str) -> Result<view::GameDetail, Error> {
        let v = self.cached_view()?;
        view::detail_in(&self.conn, &v, id)
    }

    /// The library view, built again only when the database changed.
    fn cached_view(&self) -> Result<std::sync::Arc<view::LibraryView>, Error> {
        let version: i64 = self.conn.pragma_query_value(None, "data_version", |r| r.get(0))?;
        let key = (version, self.conn.total_changes());
        let mut cache = self.view_cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((k, v)) = cache.as_ref()
            && *k == key
        {
            return Ok(v.clone());
        }
        let v = std::sync::Arc::new(view::library(&self.conn)?);
        *cache = Some((key, v.clone()));
        Ok(v)
    }

    /// Rescan every folder romforge knows (the Source and Library folders of
    /// the settings, and every scanned folder that isn't inside another),
    /// each with the scope it was scanned with. Folders that are gone are
    /// reported, not forgotten.
    pub fn refresh(
        &mut self,
        opts: ScanOptions,
        mut progress: impl FnMut(&Path, ScanProgress),
    ) -> Result<Vec<RefreshResult>, Error> {
        let roots: Vec<(String, Option<i64>, Option<i64>)> = self
            .conn
            .prepare("SELECT path, system_id, dat_id FROM scan_roots ORDER BY length(path), path")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?;
        // The Source and Library folders (and the RomForge folder's Games)
        // are scanned even before their first scan, with no scope.
        let mut roots = roots;
        let s = settings::load(&self.conn)?;
        let games = self.home.as_ref().map(home::Home::games);
        for dir in [s.source_dir, s.library_dir, games].into_iter().flatten() {
            let dir = dir.to_string_lossy().into_owned();
            if !roots.iter().any(|r| r.0 == dir) {
                roots.push((dir, None, None));
            }
        }
        roots.sort_by(|a, b| (a.0.len(), &a.0).cmp(&(b.0.len(), &b.0)));
        let mut top: Vec<&(String, Option<i64>, Option<i64>)> = Vec::new();
        for r in &roots {
            let inside = top
                .iter()
                .any(|t| Path::new(&r.0).starts_with(&t.0) && r.0 != t.0);
            if !inside {
                top.push(r);
            }
        }
        let mut out = Vec::new();
        for (path, system_id, dat_id) in top {
            let path = PathBuf::from(path);
            if !path.exists() {
                out.push(Err((path, "folder not found".into())));
                continue;
            }
            let scope = Scope::from_columns(*system_id, *dat_id);
            match self.scan_with(&path, scope, opts, |p| progress(&path, p)) {
                Ok(r) => out.push(Ok(r)),
                Err(e) => out.push(Err((path, e.to_string()))),
            }
        }
        Ok(out)
    }

    /// Games and files that need the owner (milestone 8).
    pub fn search_queue(&self, only: Option<&[String]>) -> Result<Vec<search::QueueItem>, Error> {
        search::queue(&self.conn, only)
    }

    /// Search one scraper for a row (the Search dialog's results table).
    /// `api`: Wikidata's entity search ([`search::WIKIDATA_API`]).
    pub fn search_find(
        &self,
        row_id: &str,
        scraper: search::Scraper,
        text: Option<&str>,
        system: Option<&str>,
        api: &str,
    ) -> Result<Vec<search::Found>, Error> {
        search::find(&self.conn, row_id, scraper, text, system, api)
    }

    /// The preview pane for a result: details, DAT ROMs against your file,
    /// box art.
    pub fn search_preview(
        &self,
        row_id: &str,
        found: &search::Found,
        cache_dir: &Path,
        thumbnails_base: &str,
    ) -> Result<search::Preview, Error> {
        search::preview(&self.conn, row_id, found, cache_dir, thumbnails_base)
    }

    /// Take a result for a row, then fetch for that game what `opts` asks.
    pub fn search_choose(
        &mut self,
        row_id: &str,
        found: &search::Found,
        system: Option<&str>,
        opts: &enrich::EnrichOptions,
    ) -> Result<search::Chosen, Error> {
        let opts = self.with_extdb(opts);
        search::choose(&mut self.conn, row_id, found, system, &opts)
    }

    /// Name a row's files yourself ("identified by you").
    pub fn identify_as(&mut self, row_id: &str, system: &str, title: &str) -> Result<(), Error> {
        search::identify(&mut self.conn, row_id, system, title, None)
    }

    /// Name a row's files as a translation or hack of a DAT game, listed
    /// under [`view::Derived::title`] ("translation of X", never verified).
    pub fn identify_derived(
        &mut self,
        row_id: &str,
        system: &str,
        derived: &view::Derived,
    ) -> Result<(), Error> {
        search::identify(&mut self.conn, row_id, system, &derived.title(), Some(derived))
    }

    pub fn forget_identity(&mut self, row_id: &str) -> Result<(), Error> {
        search::forget(&mut self.conn, row_id)
    }

    pub fn wikidata_candidates(&self, row_id: &str) -> Result<Vec<wikidata::Candidate>, Error> {
        search::wikidata_candidates(&self.conn, row_id)
    }

    pub fn settle_wikidata(&mut self, endpoint: &str, row_id: &str, qid: &str) -> Result<(), Error> {
        search::settle_wikidata(&mut self.conn, endpoint, row_id, qid)
    }

    pub fn settings(&self) -> Result<settings::Settings, Error> {
        settings::load(&self.conn)
    }

    /// Validate and store the settings; returns what was stored.
    pub fn save_settings(&mut self, s: &settings::Settings) -> Result<settings::Settings, Error> {
        settings::save_in(&mut self.conn, s, self.home.is_some())
    }

    /// The settings with their secrets masked, for a form (the app's).
    pub fn settings_masked(&self) -> Result<settings::Settings, Error> {
        let mut s = self.settings()?;
        s.keys = s.keys.masked();
        Ok(s)
    }

    /// Store settings from a form that got [`Library::settings_masked`]:
    /// a secret still showing the mask keeps its stored value. Returns the
    /// stored settings, masked.
    pub fn save_settings_from_form(
        &mut self,
        form: &settings::Settings,
    ) -> Result<settings::Settings, Error> {
        let stored = self.settings()?;
        let mut s = form.clone();
        s.keys = form.keys.clone().unmasked(&stored.keys);
        let mut saved = self.save_settings(&s)?;
        saved.keys = saved.keys.masked();
        Ok(saved)
    }

    pub fn systems_in_use(&self) -> Result<Vec<SystemInfo>, Error> {
        report::systems_in_use(&self.conn)
    }
}

/// The source paths of a TOSEC release's DATs for `name`, and the part
/// every release's paths for it share.
fn tosec_paths(url: &str, name: &str) -> (String, String) {
    (format!("{url}#TOSEC/{name} - "), format!("#TOSEC/{name} - "))
}

/// Remove `name`'s DATs from TOSEC releases other than `url`'s: once a
/// release is fully in, what it dropped or renamed goes too.
fn forget_older_tosec(conn: &Connection, url: &str, name: &str) -> Result<usize, Error> {
    let (prefix, part) = tosec_paths(url, name);
    let gone = conn.execute(
        "DELETE FROM dats WHERE substr(source_path, 1, length(?2)) = ?2
           AND instr(source_path, ?3) > 0 AND substr(source_path, 1, length(?1)) <> ?1",
        params![prefix, fetch::TOSEC_BASE, part],
    )?;
    if gone > 0 {
        forget_stale_serials(conn)?;
    }
    Ok(gone)
}

/// DAT serials whose game no DAT of its system has any more (the DAT was
/// replaced or removed). libretro's serials stay until its list is stored
/// again.
fn forget_stale_serials(conn: &Connection) -> Result<(), Error> {
    conn.execute(
        "DELETE FROM serials WHERE source = 'dat' AND NOT EXISTS (
           SELECT 1 FROM games g JOIN dats d ON d.dat_id = g.dat_id
           WHERE g.name = serials.title AND d.system_id = serials.system_id)",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dat(name: &str, games: &[(&str, &str)]) -> String {
        let games: String = games
            .iter()
            .map(|(g, serial)| {
                format!(r#"<game name="{g}"><serial>{serial}</serial><rom name="{g}.gb" size="1" crc="0000000{}"/></game>"#, g.len() % 10)
            })
            .collect();
        format!(r#"<datafile><header><name>{name}</name></header>{games}</datafile>"#)
    }

    fn serials(lib: &Library) -> Vec<(String, String)> {
        lib.conn
            .prepare("SELECT title, serial FROM serials ORDER BY title")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// A replaced DAT's serials go with it; a game's new serial replaces
    /// its old one.
    #[test]
    fn serials_follow_their_dat() {
        let mut lib = Library::open_in_memory().unwrap();
        let v1 = dat(
            "Nintendo - Game Boy",
            &[("Gone", "DMG-AAAA"), ("Kept", "DMG-BBBB")],
        );
        let v2 = dat(
            "Nintendo - Game Boy",
            &[("Kept", "DMG-CCCC"), ("New", "DMG-DDDD")],
        );
        lib.import_dat_bytes(v1.as_bytes(), "https://example.org/gb.dat", None)
            .unwrap();
        lib.import_dat_bytes(v2.as_bytes(), "https://example.org/gb.dat", None)
            .unwrap();
        assert_eq!(
            serials(&lib),
            [
                ("Kept".into(), "DMG-CCCC".into()),
                ("New".into(), "DMG-DDDD".into())
            ]
        );
    }

    /// Once a TOSEC release is in, a DAT it no longer has goes (and its
    /// serials); another system's TOSEC DATs stay.
    #[test]
    fn older_tosec_dats_go() {
        let mut lib = Library::open_in_memory().unwrap();
        let old = "https://www.tosecdev.org/downloads/category/58?download=1";
        let new = "https://www.tosecdev.org/downloads/category/59?download=2";
        let gb = |kind: &str| dat(&format!("Nintendo Game Boy - {kind}"), &[(kind, "DMG-EEEE")]);
        lib.import_dat_bytes(
            gb("Demos").as_bytes(),
            &format!("{old}#TOSEC/Nintendo Game Boy - Demos (TOSEC-v1).dat"),
            Some("Game Boy"),
        )
        .unwrap();
        lib.import_dat_bytes(
            gb("Games").as_bytes(),
            &format!("{new}#TOSEC/Nintendo Game Boy - Games (TOSEC-v2).dat"),
            Some("Game Boy"),
        )
        .unwrap();
        let other = dat("Nintendo Game Boy Color - Games", &[("Color", "CGB-FFFF")]);
        lib.import_dat_bytes(
            other.as_bytes(),
            &format!("{old}#TOSEC/Nintendo Game Boy Color - Games (TOSEC-v1).dat"),
            Some("Game Boy Color"),
        )
        .unwrap();
        assert_eq!(
            forget_older_tosec(&lib.conn, new, "Nintendo Game Boy").unwrap(),
            1
        );
        let names: Vec<String> = lib.summary().unwrap().dats.into_iter().map(|d| d.name).collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(!names.iter().any(|n| n.ends_with("Demos")), "{names:?}");
        assert!(serials(&lib).iter().all(|(t, _)| t != "Demos"));
    }
}
