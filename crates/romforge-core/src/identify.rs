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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Via {
    Sha1,
    Md5,
    Crc32,
}

impl Via {
    fn parse(s: &str) -> Option<Via> {
        match s {
            "sha1" => Some(Via::Sha1),
            "md5" => Some(Via::Md5),
            "crc32" => Some(Via::Crc32),
            _ => None,
        }
    }

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
    game_id: i64,
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
    game_id: i64,
    via: Via,
    stripped: bool,
    in_scope: bool,
}

fn lookup(conn: &Connection, d: &Digest) -> Result<Vec<RomRow>, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT r.rom_id, g.dat_id, d.system_id, r.size, r.crc32, r.md5, r.sha1, r.game_id
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
            game_id: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

struct FileRow {
    file_id: i64,
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

/// Recompute matches and statuses. Files in `relook` are looked up in the
/// DATs again; every file's status (and whether each match is in its
/// folder's scope) is worked out again from its matches, since which copy
/// is the duplicate depends on all of them: the most complete one wins.
pub fn identify(tx: &Transaction, relook: Relook) -> Result<(), Error> {
    let files: Vec<FileRow> = {
        let mut stmt = tx.prepare(
            "SELECT f.file_id, s.system_id, s.dat_id, f.size, f.crc32, f.md5, f.sha1,
                    f.stripped_size, f.stripped_crc32, f.stripped_md5, f.stripped_sha1, f.status,
                    f.path, f.member
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
            let (path, member): (String, String) = (r.get(12)?, r.get(13)?);
            let place = if member.is_empty() {
                path.rsplit_once('/')
                    .map_or(String::new(), |(dir, _)| dir.to_owned())
            } else {
                path
            };
            Ok(FileRow {
                file_id: r.get(0)?,
                place,
                scope: Scope::from_columns(r.get(1)?, r.get(2)?),
                raw: Digest {
                    size: r.get(3)?,
                    crc32: r.get(4)?,
                    md5: r.get(5)?,
                    sha1: r.get(6)?,
                },
                stripped,
                status: r.get(11)?,
            })
        })?
        .collect::<Result<_, _>>()?
    };

    // Files the owner named in Search, by content.
    let manual: std::collections::HashSet<(String, u64)> = tx
        .prepare("SELECT sha1, size FROM manual_ids")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    // Sheet matches are worked out again below.
    tx.execute("DELETE FROM matches WHERE via = 'sheet'", [])?;
    let relook_all = matches!(relook, Relook::All);
    if relook_all {
        tx.execute("DELETE FROM matches", [])?;
    }
    let mut insert = tx.prepare(
        "INSERT INTO matches (file_id, rom_id, via, stripped, out_of_scope) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    let mut drop_matches = tx.prepare("DELETE FROM matches WHERE file_id = ?1")?;
    let mut set_scope =
        tx.prepare("UPDATE matches SET out_of_scope = ?3 WHERE file_id = ?1 AND rom_id = ?2")?;
    let mut set_status = tx.prepare("UPDATE files SET status = ?2 WHERE file_id = ?1")?;
    // The stored matches of the files not looked up again, with what decides
    // their scope (their ROM's DAT and system).
    let mut stored: HashMap<i64, Vec<(RomRow, Via, bool, bool)>> = HashMap::new();
    if !relook_all {
        let mut stmt = tx.prepare(
            "SELECT m.file_id, m.rom_id, m.via, m.stripped, m.out_of_scope, g.dat_id, d.system_id,
                    r.game_id
             FROM matches m JOIN roms r ON r.rom_id = m.rom_id
             JOIN games g ON g.game_id = r.game_id JOIN dats d ON d.dat_id = g.dat_id",
        )?;
        for row in stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, bool>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, i64>(7)?,
            ))
        })? {
            let (file, rom_id, via, stripped, out, dat_id, system_id, game_id) = row?;
            let Some(via) = Via::parse(&via) else { continue };
            let rom = RomRow {
                rom_id,
                game_id,
                dat_id,
                system_id,
                size: None,
                crc32: None,
                md5: None,
                sha1: None,
            };
            stored.entry(file).or_default().push((rom, via, stripped, out));
        }
    }
    // First every file's hits, then who owns which ROM.
    let mut all_hits: Vec<Vec<Hit>> = Vec::with_capacity(files.len());
    for file in &files {
        let again = match relook {
            Relook::All => true,
            Relook::Files(ids) => ids.contains(&file.file_id),
            Relook::Nothing => false,
        };
        let mut hits: Vec<Hit> = Vec::new();
        if again {
            if !relook_all {
                drop_matches.execute([file.file_id])?;
            }
            let mut seen = HashSet::new();
            let digests = std::iter::once((&file.raw, false)).chain(file.stripped.iter().map(|d| (d, true)));
            // An empty file says nothing: TOSEC lists many empty ROMs.
            for (digest, stripped) in digests.filter(|(d, _)| d.size > 0) {
                for rom in lookup(tx, digest)? {
                    if let Some(via) = compare(digest, &rom)
                        && seen.insert(rom.rom_id)
                    {
                        hits.push(Hit {
                            rom_id: rom.rom_id,
                            game_id: rom.game_id,
                            via,
                            stripped,
                            in_scope: file.scope.contains(&rom),
                        });
                    }
                }
            }
            for h in &hits {
                insert.execute(params![
                    file.file_id,
                    h.rom_id,
                    h.via.as_str(),
                    h.stripped,
                    !h.in_scope
                ])?;
            }
        } else {
            for (rom, via, stripped, was_out) in stored.remove(&file.file_id).unwrap_or_default() {
                let in_scope = file.scope.contains(&rom);
                // The folder's scope may have changed since.
                if in_scope == was_out {
                    set_scope.execute(params![file.file_id, rom.rom_id, !in_scope])?;
                }
                hits.push(Hit {
                    rom_id: rom.rom_id,
                    game_id: rom.game_id,
                    via,
                    stripped,
                    in_scope,
                });
            }
        }
        all_hits.push(hits);
    }

    // The copy that holds more of a game in one place (an archive or a
    // folder) owns its ROMs; the others are duplicates. Ties go by path.
    let mut held: HashMap<(&str, i64), HashSet<i64>> = HashMap::new();
    for (file, hits) in files.iter().zip(&all_hits) {
        for h in hits.iter().filter(|h| h.in_scope && h.via.strong()) {
            held.entry((file.place.as_str(), h.game_id))
                .or_default()
                .insert(h.rom_id);
        }
    }
    let mut order: Vec<(usize, usize)> = files
        .iter()
        .zip(&all_hits)
        .enumerate()
        .map(|(i, (file, hits))| {
            let most = hits
                .iter()
                .filter_map(|h| held.get(&(file.place.as_str(), h.game_id)))
                .map(HashSet::len)
                .max()
                .unwrap_or(0);
            (i, most)
        })
        .collect();
    order.sort_by_key(|&(i, most)| (std::cmp::Reverse(most), i));
    // ROM id -> file that claimed it.
    let mut owners: HashMap<i64, i64> = HashMap::new();
    for (i, _) in order {
        let (file, hits) = (&files[i], &all_hits[i]);

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
        } else if manual.contains(&(file.raw.sha1.clone(), file.raw.size)) {
            // The owner's pick outranks weak evidence (a CRC32-only hit, a
            // hash hit outside the folder's scope); only a match in scope
            // comes first.
            Status::Manual
        } else if hits.iter().any(|h| h.in_scope) {
            Status::Candidate
        } else if hits.iter().any(|h| h.via.strong()) {
            Status::WrongSystem
        } else {
            Status::Unknown
        };
        if status.as_str() != file.status {
            set_status.execute(params![file.file_id, status.as_str()])?;
        }
    }
    fit_sheets(tx)?;
    Ok(())
}

/// Unknown cue (or GDI) sheets whose files are exactly a game's tracks (in
/// order), every one hash-matched in the same archive or folder: status
/// `sheet`, with a match to the game's cue ROM (`via` = 'sheet', which
/// never counts as had).
fn fit_sheets(tx: &Transaction) -> Result<(), Error> {
    let sheets: Vec<(i64, String, String, String)> = tx
        .prepare(
            "SELECT file_id, path, member, sheet_files FROM files
             WHERE status = 'unknown' AND sheet_files IS NOT NULL",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    if sheets.is_empty() {
        return Ok(());
    }
    // Hash-matched files beside it: in the same archive, or loose in the
    // same folder.
    let mut beside_archive = tx.prepare(concat!(
        "SELECT file_id, member FROM files WHERE path = ?1 AND member <> ''
           AND status IN ",
        crate::report::holding!()
    ))?;
    let mut beside_folder = tx.prepare(concat!(
        "SELECT file_id, path FROM files WHERE member = '' AND status IN ",
        crate::report::holding!(),
        "
           AND substr(path, 1, length(?1) + 1) = ?1 || '/' AND instr(substr(path, length(?1) + 2), '/') = 0"
    ))?;
    let mut games_of = tx.prepare(concat!(
        "SELECT DISTINCT r.game_id FROM matches m JOIN roms r ON r.rom_id = m.rom_id
         WHERE m.file_id = ?1 AND m.via IN ",
        crate::report::strong!(),
        " AND m.out_of_scope = 0"
    ))?;
    let mut roms_of = tx.prepare(
        "SELECT rom_id, name, lower(name) LIKE '%.cue' FROM roms
         WHERE game_id = ?1 AND coalesce(status, '') <> 'nodump' ORDER BY rom_id",
    )?;
    let mut matched_by = tx.prepare(concat!(
        "SELECT 1 FROM matches WHERE rom_id = ?1 AND file_id = ?2 AND via IN ",
        crate::report::strong!()
    ))?;
    let mut insert = tx.prepare(
        "INSERT INTO matches (file_id, rom_id, via, stripped, out_of_scope) VALUES (?1, ?2, 'sheet', 0, 0)",
    )?;
    let mut set_status = tx.prepare("UPDATE files SET status = 'sheet' WHERE file_id = ?1")?;
    for (file_id, path, member, names) in sheets {
        let Ok(names) = serde_json::from_str::<Vec<String>>(&names) else {
            continue;
        };
        let beside: Vec<i64> = if member.is_empty() {
            let dir = std::path::Path::new(&path)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            beside_folder
                .query_map([&dir], |r| r.get(0))?
                .collect::<Result<_, _>>()?
        } else {
            beside_archive
                .query_map([&path], |r| r.get(0))?
                .collect::<Result<_, _>>()?
        };
        let mut games = std::collections::BTreeSet::new();
        for f in &beside {
            for g in games_of.query_map([f], |r| r.get::<_, i64>(0))? {
                games.insert(g?);
            }
        }
        for game in games {
            let roms: Vec<(i64, String, bool)> = roms_of
                .query_map([game], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<_, _>>()?;
            let tracks: Vec<&(i64, String, bool)> = roms.iter().filter(|r| !r.2).collect();
            let fits = tracks.len() == names.len()
                && tracks
                    .iter()
                    .zip(&names)
                    .all(|(t, n)| t.1.eq_ignore_ascii_case(n))
                && tracks.iter().all(|t| {
                    beside
                        .iter()
                        .any(|f| matched_by.exists(params![t.0, f]).unwrap_or(false))
                });
            if !fits {
                continue;
            }
            for cue in roms.iter().filter(|r| r.2) {
                insert.execute(params![file_id, cue.0])?;
            }
            set_status.execute([file_id])?;
            break;
        }
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
