//! romforge core: DAT import, scanning, hashing and identification.
//!
//! Scanning and identifying never touch ROM files. Only [`organize`]
//! (milestone 2) moves and renames them, and only [`cleanup`] (milestone
//! 10) repacks them, each after a dry run and journaled for undo.

pub mod artsources;
mod atomic;
pub mod cleanup;
pub mod controller;
pub mod dat;
mod db;
pub mod disc;
pub mod emusettings;
pub mod enrich;
pub mod extdb;
pub mod fetch;
pub mod firmware;
pub mod hash;
pub mod headers;
pub mod home;
pub mod hotkeys;
pub mod ident;
mod identify;
pub mod job;
pub mod launch;
pub mod layout;
mod library;
pub mod manual;
pub mod names;
pub mod obtain;
pub mod organize;
pub mod parallel;
pub mod patch;
pub mod patcher;
mod report;
pub mod scan;
pub mod scummvm;
pub mod search;
pub mod settings;
pub mod suggest;
pub mod systems;
pub mod thumbs;
pub mod titles;
mod vcdiff;
pub mod view;
pub mod wikidata;

use std::path::PathBuf;

use rusqlite::Connection;

pub use identify::{Scope, Status};
pub use library::{
    DatState, DatStatus, FolderScan, ImportedDat, ListOptions, ListsReport, PlatformIcons, PreviewGame,
    RefreshResult, ScanPreview, ScanProgress, ScanReport, ScummvmList, SkippedKind,
};
pub use report::{DatGame, DatSummary, LibraryFile, LibraryFilter, Summary, SystemInfo};
pub use scan::ScanOptions;

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
    #[error("picture: {0}")]
    Image(String),
    /// Stopped at a safe point because it was asked to ([`job::Cancel`]).
    #[error("stopped")]
    Cancelled,
    /// A quick edit ([`Library::edit`]) found the library held by another
    /// connection's long write. The message starts with `busy:` so a
    /// caller can tell it apart and wait for the job instead.
    #[error("busy: the library is held by a running scan, refresh or DAT import until it ends")]
    Busy,
}

impl Error {
    /// Whether SQLite gave up waiting for another connection's write.
    pub fn is_busy(&self) -> bool {
        use rusqlite::ErrorCode::{DatabaseBusy, DatabaseLocked};
        match self {
            Error::Busy => true,
            Error::Sql(rusqlite::Error::SqliteFailure(e, _)) => {
                matches!(e.code, DatabaseBusy | DatabaseLocked)
            }
            _ => false,
        }
    }
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
    /// Where the downloaded databases and lists are, when not the usual
    /// place (Scan folder's dry run reads the library's own on its copy).
    databases: Option<PathBuf>,
    /// The library view, kept until the database changes (by any
    /// connection: SQLite's `data_version`, and this one's own changes).
    view_cache: std::sync::Mutex<Option<CachedView>>,
}

/// ((SQLite data_version, this connection's total changes), the view's
/// fingerprint, the view): nothing written since, or nothing the view
/// shows changed, and the view stands.
type CachedView = ((i64, u64), u64, std::sync::Arc<view::LibraryView>);
