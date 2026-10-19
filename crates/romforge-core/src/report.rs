//! Read-only queries for the CLI and the UI.

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::Serialize;

use crate::Error;
use crate::identify::Status;

/// The match kinds that verify (house rule 8: size + SHA1, or size + MD5;
/// `confirmed`: an arcade ROM known only by CRC32 whose file's SHA1 fits
/// the same ROM in another DAT), as an SQL set: `m.via IN strong!()`. A
/// CRC32 hit (`crc32`) or a fitting sheet (`sheet`) never does.
macro_rules! strong {
    () => {
        "('sha1', 'md5', 'confirmed')"
    };
}

/// The file statuses that hold a verified copy of what they matched (the
/// copy that counts, or another copy of it), as an SQL set:
/// `f.status IN holding!()`.
macro_rules! holding {
    () => {
        "('matched', 'duplicate')"
    };
}
pub(crate) use {holding, strong};

/// ROMs that a matched (or duplicate) file hit by size + SHA1/MD5 inside
/// that file's scan scope.
pub(crate) const HAD: &str = concat!(
    "
    had AS (
        SELECT DISTINCT m.rom_id FROM matches m JOIN files f ON f.file_id = m.file_id
        WHERE m.out_of_scope = 0 AND m.via IN ",
    strong!(),
    "
          AND f.status IN ",
    holding!(),
    "
    )"
);

/// Per game in `games_in` (a CTE of `game_id`s): how many of its data ROMs
/// and cue sheets are had. `nodump` ROMs are not counted. Only games with a
/// had ROM can be complete, so callers keep `games_in` to those where they
/// can; scanning every ROM of every DAT is slow at TOSEC scale.
pub(crate) const GAME_HAVE: &str = "
    game_have AS (
        SELECT r.game_id,
               sum(NOT r.sheet) AS data_total,
               sum(NOT r.sheet AND h.rom_id IS NOT NULL) AS data_have,
               sum(r.sheet) AS sheet_total,
               sum(r.sheet AND h.rom_id IS NOT NULL) AS sheet_have
        FROM (SELECT rom_id, game_id, lower(name) LIKE '%.cue' AS sheet FROM roms
              WHERE game_id IN (SELECT game_id FROM games_in)
                AND coalesce(status, '') NOT IN ('nodump', 'optional')) r
        LEFT JOIN had h ON h.rom_id = r.rom_id
        GROUP BY r.game_id
    )";

/// A game is "have" when every data ROM is had. Cue sheets are text that
/// tools rewrite freely, so one that differs from the DAT doesn't make a
/// disc missing (the game is flagged `cue_differs` instead); a game made only
/// of sheets needs them all.
pub(crate) const IS_HAVE: &str = "
    (CASE WHEN gh.data_total > 0 THEN gh.data_have = gh.data_total
          ELSE gh.sheet_total > 0 AND gh.sheet_have = gh.sheet_total END)";

/// The matches that say what a file is, by its status: SHA1/MD5 hits in
/// scope for a matched or duplicate file, its CRC32 hits for a candidate,
/// out-of-scope SHA1/MD5 hits for `wrong_system`, the sheet match for a
/// `sheet`. An unknown file has none: a CRC32 alone never names it (house
/// rule 8). Needs `f` (files) and `m` (matches).
pub(crate) const RELEVANT_MATCH: &str = concat!(
    concat!(
        "
    ((f.status IN ",
        holding!(),
        " AND m.out_of_scope = 0 AND m.via IN "
    ),
    strong!(),
    ")
     OR (f.status = 'candidate' AND m.out_of_scope = 0)
     OR (f.status = 'wrong_system' AND m.out_of_scope = 1 AND m.via IN ",
    strong!(),
    ")
     OR (f.status = 'sheet' AND m.via = 'sheet'))"
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DatSummary {
    pub dat_id: i64,
    pub name: String,
    pub version: Option<String>,
    pub system: Option<String>,
    pub header_hint: Option<String>,
    pub source_path: String,
    pub imported_at: String,
    pub games: i64,
    pub have: i64,
    pub missing: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub files_by_status: Vec<(Status, i64)>,
    pub dats: Vec<DatSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DatGame {
    pub game_id: i64,
    pub name: String,
    pub have: bool,
    /// Have, but a cue sheet differs from the DAT's.
    pub cue_differs: bool,
    pub roms_total: i64,
    pub roms_have: i64,
    /// Files that provide this game's ROMs (`path` or `path#member`).
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryFilter {
    pub system_id: Option<i64>,
    pub status: Option<Status>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryFile {
    pub file_id: i64,
    pub path: String,
    pub member: String,
    pub size: u64,
    pub status: Status,
    pub header_kind: Option<String>,
    pub games: Vec<String>,
    pub systems: Vec<String>,
    #[serde(skip)]
    system_ids: Vec<i64>,
}

impl LibraryFile {
    /// `path` for plain files, `path#member` for archive members.
    pub fn display_path(&self) -> String {
        display_path(&self.path, &self.member)
    }
}

fn display_path(path: &str, member: &str) -> String {
    if member.is_empty() {
        path.to_owned()
    } else {
        format!("{path}#{member}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemInfo {
    pub system_id: i64,
    pub key: String,
    pub name: String,
    pub dats: i64,
}

fn status(s: String) -> rusqlite::Result<Status> {
    Status::parse(&s).ok_or_else(|| rusqlite::Error::InvalidColumnType(0, s, rusqlite::types::Type::Text))
}

pub fn summary(conn: &Connection) -> Result<Summary, Error> {
    let mut counts: BTreeMap<String, i64> = BTreeMap::new();
    let mut stmt = conn.prepare("SELECT status, count(*) FROM files GROUP BY status")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        let (s, n) = row?;
        counts.insert(s, n);
    }
    let files_by_status = Status::ALL
        .into_iter()
        .map(|s| (s, counts.get(s.as_str()).copied().unwrap_or(0)))
        .collect();

    let mut stmt = conn.prepare(&format!(
        "WITH {HAD},
              games_in AS (SELECT DISTINCT r.game_id FROM had JOIN roms r ON r.rom_id = had.rom_id),
              {GAME_HAVE},
              dat_have AS (
                  SELECT g.dat_id, sum({IS_HAVE}) AS have
                  FROM game_have gh JOIN games g ON g.game_id = gh.game_id GROUP BY g.dat_id
              )
         SELECT d.dat_id, d.name, d.version, s.key, d.header_hint, d.source_path, d.imported_at,
                (SELECT count(*) FROM games g WHERE g.dat_id = d.dat_id),
                coalesce(dh.have, 0)
         FROM dats d
         LEFT JOIN systems s ON s.system_id = d.system_id
         LEFT JOIN dat_have dh ON dh.dat_id = d.dat_id
         ORDER BY d.name, d.dat_id"
    ))?;
    let dats = stmt
        .query_map([], |r| {
            let games: i64 = r.get(7)?;
            let have: i64 = r.get(8)?;
            Ok(DatSummary {
                dat_id: r.get(0)?,
                name: r.get(1)?,
                version: r.get(2)?,
                system: r.get(3)?,
                header_hint: r.get(4)?,
                source_path: r.get(5)?,
                imported_at: r.get(6)?,
                games,
                have,
                missing: games - have,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Summary {
        files_by_status,
        dats,
    })
}

pub fn dat_games(conn: &Connection, dat_id: i64) -> Result<Vec<DatGame>, Error> {
    let mut files: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut stmt = conn.prepare(concat!(
        "SELECT DISTINCT r.game_id, f.path, f.member
         FROM matches m JOIN files f ON f.file_id = m.file_id JOIN roms r ON r.rom_id = m.rom_id
         JOIN games g ON g.game_id = r.game_id
         WHERE g.dat_id = ?1 AND m.out_of_scope = 0 AND m.via IN ",
        strong!(),
        "
           AND f.status IN ",
        holding!(),
        "
         ORDER BY f.path, f.member"
    ))?;
    for row in stmt.query_map([dat_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (game, path, member) = row?;
        files.entry(game).or_default().push(display_path(&path, &member));
    }

    let mut stmt = conn.prepare(&format!(
        "WITH {HAD},
              games_in AS (SELECT game_id FROM games WHERE dat_id = ?1),
              {GAME_HAVE}
         SELECT g.game_id, g.name,
                coalesce(gh.data_total + gh.sheet_total, 0),
                coalesce(gh.data_have + gh.sheet_have, 0),
                coalesce({IS_HAVE}, 0),
                coalesce(gh.sheet_have < gh.sheet_total, 0)
         FROM games g LEFT JOIN game_have gh ON gh.game_id = g.game_id
         WHERE g.dat_id = ?1 ORDER BY g.name, g.game_id"
    ))?;
    let games = stmt
        .query_map([dat_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, bool>(5)?,
            ))
        })?
        .map(|row| {
            let (game_id, name, roms_total, roms_have, have, sheets_differ) = row?;
            Ok(DatGame {
                game_id,
                name,
                have,
                cue_differs: have && sheets_differ,
                roms_total,
                roms_have,
                files: files.remove(&game_id).unwrap_or_default(),
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(games)
}

pub fn files(conn: &Connection, filter: &LibraryFilter) -> Result<Vec<LibraryFile>, Error> {
    // file_id -> (game names, system keys, system ids)
    type Found = (Vec<String>, Vec<String>, Vec<i64>);
    let mut by_id: BTreeMap<i64, Found> = BTreeMap::new();
    let mut stmt = conn.prepare(&format!(
        "SELECT DISTINCT f.file_id, g.name, s.key, s.system_id
         FROM files f JOIN matches m ON m.file_id = f.file_id
         JOIN roms r ON r.rom_id = m.rom_id JOIN games g ON g.game_id = r.game_id
         JOIN dats d ON d.dat_id = g.dat_id LEFT JOIN systems s ON s.system_id = d.system_id
         WHERE {RELEVANT_MATCH}
         ORDER BY g.name"
    ))?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<i64>>(3)?,
        ))
    })? {
        let (file_id, game, system, system_id) = row?;
        let entry = by_id.entry(file_id).or_default();
        if !entry.0.contains(&game) {
            entry.0.push(game);
        }
        if let (Some(key), Some(id)) = (system, system_id)
            && !entry.2.contains(&id)
        {
            entry.1.push(key);
            entry.2.push(id);
        }
    }

    let mut stmt = conn.prepare(
        "SELECT file_id, path, member, size, status, header_kind FROM files ORDER BY path, member",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(LibraryFile {
            file_id: r.get(0)?,
            path: r.get(1)?,
            member: r.get(2)?,
            size: r.get(3)?,
            status: status(r.get(4)?)?,
            header_kind: r.get(5)?,
            games: Vec::new(),
            systems: Vec::new(),
            system_ids: Vec::new(),
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        let mut file = row?;
        if let Some((games, systems, ids)) = by_id.remove(&file.file_id) {
            file.games = games;
            file.systems = systems;
            file.system_ids = ids;
        }
        if filter.status.is_some_and(|s| s != file.status) {
            continue;
        }
        if filter.system_id.is_some_and(|s| !file.system_ids.contains(&s)) {
            continue;
        }
        out.push(file);
    }
    Ok(out)
}

pub fn systems_in_use(conn: &Connection) -> Result<Vec<SystemInfo>, Error> {
    let mut stmt = conn.prepare(
        "SELECT s.system_id, s.key, s.name, count(d.dat_id)
         FROM systems s JOIN dats d ON d.system_id = s.system_id
         GROUP BY s.system_id ORDER BY s.name",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SystemInfo {
                system_id: r.get(0)?,
                key: r.get(1)?,
                name: r.get(2)?,
                dats: r.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}
