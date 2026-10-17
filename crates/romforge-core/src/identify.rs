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
}

impl Status {
    pub const ALL: [Status; 6] = [
        Status::Matched,
        Status::Candidate,
        Status::Unknown,
        Status::WrongSystem,
        Status::Duplicate,
        Status::Manual,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Matched => "matched",
            Status::Candidate => "candidate",
            Status::Unknown => "unknown",
            Status::WrongSystem => "wrong_system",
            Status::Duplicate => "duplicate",
            Status::Manual => "manual",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Status::ALL.into_iter().find(|st| st.as_str() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Via {
    Sha1,
    Md5,
    Crc32,
}

impl Via {
    fn as_str(self) -> &'static str {
        match self {
            Via::Sha1 => "sha1",
            Via::Md5 => "md5",
            Via::Crc32 => "crc32",
        }
    }

    fn strong(self) -> bool {
        self != Via::Crc32
    }
}

struct RomRow {
    rom_id: i64,
    dat_id: i64,
    system_id: Option<i64>,
    size: Option<u64>,
    crc32: Option<String>,
    md5: Option<String>,
    sha1: Option<String>,
}

fn compare(d: &Digest, rom: &RomRow) -> Option<Via> {
    if rom.size != Some(d.size) {
        return None;
    }
    if let Some(sha1) = &rom.sha1 {
        return (*sha1 == d.sha1).then_some(Via::Sha1);
    }
    if let Some(md5) = &rom.md5 {
        return (*md5 == d.md5).then_some(Via::Md5);
    }
    (rom.crc32.as_deref() == Some(d.crc32.as_str())).then_some(Via::Crc32)
}

struct Hit {
    rom_id: i64,
    via: Via,
    stripped: bool,
    in_scope: bool,
}

fn lookup(conn: &Connection, d: &Digest) -> Result<Vec<RomRow>, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT r.rom_id, g.dat_id, d.system_id, r.size, r.crc32, r.md5, r.sha1
         FROM roms r JOIN games g ON g.game_id = r.game_id JOIN dats d ON d.dat_id = g.dat_id
         WHERE r.sha1 = ?1 OR r.md5 = ?2 OR r.crc32 = ?3",
    )?;
    let rows = stmt.query_map(params![d.sha1, d.md5, d.crc32], |r| {
        Ok(RomRow {
            rom_id: r.get(0)?,
            dat_id: r.get(1)?,
            system_id: r.get(2)?,
            size: r.get(3)?,
            crc32: r.get(4)?,
            md5: r.get(5)?,
            sha1: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

struct FileRow {
    file_id: i64,
    scope: Scope,
    raw: Digest,
    stripped: Option<Digest>,
}

/// Recompute matches and statuses for every scanned file.
pub fn identify_all(tx: &Transaction) -> Result<(), Error> {
    let files: Vec<FileRow> = {
        let mut stmt = tx.prepare(
            "SELECT f.file_id, s.system_id, s.dat_id, f.size, f.crc32, f.md5, f.sha1,
                    f.stripped_size, f.stripped_crc32, f.stripped_md5, f.stripped_sha1
             FROM files f JOIN scan_roots s ON s.root_id = f.root_id
             ORDER BY f.path, f.member",
        )?;
        stmt.query_map([], |r| {
            let stripped = match r.get::<_, Option<u64>>(7)? {
                Some(size) => Some(Digest {
                    size,
                    crc32: r.get(8)?,
                    md5: r.get(9)?,
                    sha1: r.get(10)?,
                }),
                None => None,
            };
            Ok(FileRow {
                file_id: r.get(0)?,
                scope: Scope::from_columns(r.get(1)?, r.get(2)?),
                raw: Digest {
                    size: r.get(3)?,
                    crc32: r.get(4)?,
                    md5: r.get(5)?,
                    sha1: r.get(6)?,
                },
                stripped,
            })
        })?
        .collect::<Result<_, _>>()?
    };

    // Files the owner named in Search, by content.
    let manual: std::collections::HashSet<(String, u64)> = tx
        .prepare("SELECT sha1, size FROM manual_ids")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    tx.execute("DELETE FROM matches", [])?;
    let mut insert = tx.prepare(
        "INSERT INTO matches (file_id, rom_id, via, stripped, out_of_scope) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    let mut set_status = tx.prepare("UPDATE files SET status = ?2 WHERE file_id = ?1")?;
    // ROM id -> file that first claimed it (files are in path order).
    let mut owners: HashMap<i64, i64> = HashMap::new();

    for file in &files {
        let mut hits: Vec<Hit> = Vec::new();
        let mut seen = HashSet::new();
        let digests = std::iter::once((&file.raw, false)).chain(file.stripped.iter().map(|d| (d, true)));
        for (digest, stripped) in digests {
            for rom in lookup(tx, digest)? {
                if let Some(via) = compare(digest, &rom)
                    && seen.insert(rom.rom_id)
                {
                    hits.push(Hit {
                        rom_id: rom.rom_id,
                        via,
                        stripped,
                        in_scope: file.scope.contains(&rom),
                    });
                }
            }
        }

        let strong_in_scope: Vec<i64> = hits
            .iter()
            .filter(|h| h.in_scope && h.via.strong())
            .map(|h| h.rom_id)
            .collect();
        let status = if !strong_in_scope.is_empty() {
            if strong_in_scope.iter().all(|r| owners.contains_key(r)) {
                Status::Duplicate
            } else {
                for r in &strong_in_scope {
                    owners.entry(*r).or_insert(file.file_id);
                }
                Status::Matched
            }
        } else if hits.iter().any(|h| h.in_scope) {
            Status::Candidate
        } else if hits.iter().any(|h| h.via.strong()) {
            Status::WrongSystem
        } else if manual.contains(&(file.raw.sha1.clone(), file.raw.size)) {
            Status::Manual
        } else {
            Status::Unknown
        };

        for h in &hits {
            insert.execute(params![
                file.file_id,
                h.rom_id,
                h.via.as_str(),
                h.stripped,
                !h.in_scope
            ])?;
        }
        set_status.execute(params![file.file_id, status.as_str()])?;
    }
    Ok(())
}

pub(crate) fn resolve_dat(conn: &Connection, dat_id: i64) -> Result<i64, Error> {
    conn.query_row("SELECT dat_id FROM dats WHERE dat_id = ?1", [dat_id], |r| {
        r.get(0)
    })
    .optional()?
    .ok_or(Error::UnknownDat(dat_id))
}
