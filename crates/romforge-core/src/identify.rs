//! Matching scanned files against imported DATs.
//!
//! - size + SHA1, or size + MD5 when the DAT has no SHA1 → a real hit;
//! - size + CRC32 when the DAT has neither SHA1 nor MD5 → a candidate only;
//! - a stronger hash that disagrees rules the ROM out, whatever the CRC says.
//!
//! Every hit is recorded; nothing is picked with `LIMIT 1`.

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::Error;
use crate::hash::Digest;

mod lookup;
use lookup::*;
mod run;
pub use run::*;
mod sheets;
use sheets::*;

/// Which DATs a scanned folder is identified against. There is no default
/// system: `All` means every imported DAT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    All,
    System(i64),
    Dat(i64),
}

impl Scope {
    pub(crate) fn from_columns(system_id: Option<i64>, dat_id: Option<i64>) -> Self {
        match (dat_id, system_id) {
            (Some(d), _) => Scope::Dat(d),
            (None, Some(s)) => Scope::System(s),
            (None, None) => Scope::All,
        }
    }

    fn contains(self, hit: &RomRow) -> bool {
        match self {
            Scope::All => true,
            Scope::System(s) => hit.system_id == Some(s),
            Scope::Dat(d) => hit.dat_id == d,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Matched,
    Candidate,
    Unknown,
    WrongSystem,
    Duplicate,
    /// No hash match; the owner named it in Search (house rule 8: never
    /// counted as verified).
    Manual,
    /// A cue sheet with no hash match whose FILE lines name exactly a
    /// game's tracks, all hash-matched beside it (a rewritten sheet; never
    /// counted as verified).
    Sheet,
}

impl Status {
    pub const ALL: [Status; 7] = [
        Status::Matched,
        Status::Candidate,
        Status::Unknown,
        Status::WrongSystem,
        Status::Duplicate,
        Status::Manual,
        Status::Sheet,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Matched => "matched",
            Status::Candidate => "candidate",
            Status::Unknown => "unknown",
            Status::WrongSystem => "wrong_system",
            Status::Duplicate => "duplicate",
            Status::Manual => "manual",
            Status::Sheet => "sheet",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Status::ALL.into_iter().find(|st| st.as_str() == s)
    }
}

struct FileRow {
    file_id: i64,
    /// A CHD's own SHA1 (from its header): an arcade DAT's disk.
    chd_sha1: Option<String>,
    /// The archive it is in, or its folder: where its game's other files are.
    place: String,
    scope: Scope,
    raw: Digest,
    stripped: Option<Digest>,
    status: String,
}

/// Which files [`identify`] looks up in the DATs again; the others keep
/// their stored matches (still valid until their hashes or the DATs change).
pub enum Relook<'a> {
    /// Every file: DATs were imported or removed.
    All,
    /// These files were hashed again.
    Files(&'a HashSet<i64>),
    /// None: only the owner's picks changed.
    Nothing,
}

/// [`identify`] looking every file up again.
pub fn identify_all(tx: &Transaction) -> Result<(), Error> {
    identify(tx, Relook::All)
}

pub(crate) fn resolve_dat(conn: &Connection, dat_id: i64) -> Result<i64, Error> {
    conn.query_row("SELECT dat_id FROM dats WHERE dat_id = ?1", [dat_id], |r| {
        r.get(0)
    })
    .optional()?
    .ok_or(Error::UnknownDat(dat_id))
}
