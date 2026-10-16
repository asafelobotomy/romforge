//! Read-only queries for the CLI and the UI.

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::Serialize;

use crate::Error;
use crate::identify::Status;

/// A ROM counts as "have" when a matched (or duplicate) file hit it by size +
/// SHA1/MD5 inside that file's scan scope. `nodump` ROMs are not counted.
const ROM_HAVE: &str = "
    rom_have AS (
        SELECT r.rom_id, r.game_id, EXISTS (
            SELECT 1 FROM matches m JOIN files f ON f.file_id = m.file_id
            WHERE m.rom_id = r.rom_id AND m.out_of_scope = 0 AND m.via IN ('sha1', 'md5')
              AND f.status IN ('matched', 'duplicate')
        ) AS have
        FROM roms r WHERE coalesce(r.status, '') <> 'nodump'
    ),
    game_have AS (
        SELECT g.game_id, g.dat_id, g.name, count(rh.rom_id) AS total,
               coalesce(sum(rh.have), 0) AS have_roms
        FROM games g LEFT JOIN rom_have rh ON rh.game_id = g.game_id
        GROUP BY g.game_id
    )";

/// The matches that explain a file's status: in-scope hits, or for
/// `wrong_system` the out-of-scope SHA1/MD5 hits.
const RELEVANT_MATCH: &str = "
    ((f.status = 'wrong_system' AND m.out_of_scope = 1 AND m.via <> 'crc32')
     OR (f.status <> 'wrong_system' AND m.out_of_scope = 0))";

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
        "WITH {ROM_HAVE}
         SELECT d.dat_id, d.name, d.version, s.key, d.header_hint, d.source_path, d.imported_at,
                count(gh.game_id),
                coalesce(sum(gh.total > 0 AND gh.have_roms = gh.total), 0)
         FROM dats d
         LEFT JOIN systems s ON s.system_id = d.system_id
         LEFT JOIN game_have gh ON gh.dat_id = d.dat_id
         GROUP BY d.dat_id ORDER BY d.name, d.dat_id"
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
    let mut stmt = conn.prepare(
        "SELECT DISTINCT r.game_id, f.path, f.member
         FROM matches m JOIN files f ON f.file_id = m.file_id JOIN roms r ON r.rom_id = m.rom_id
         JOIN games g ON g.game_id = r.game_id
         WHERE g.dat_id = ?1 AND m.out_of_scope = 0 AND m.via IN ('sha1', 'md5')
           AND f.status IN ('matched', 'duplicate')
         ORDER BY f.path, f.member",
    )?;
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
        "WITH {ROM_HAVE}
         SELECT game_id, name, total, have_roms FROM game_have WHERE dat_id = ?1 ORDER BY name, game_id"
    ))?;
    let games = stmt
        .query_map([dat_id], |r| {
            let total: i64 = r.get(2)?;
            let have: i64 = r.get(3)?;
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, total, have))
        })?
        .map(|row| {
            let (game_id, name, roms_total, roms_have) = row?;
            Ok(DatGame {
                game_id,
                name,
                have: roms_total > 0 && roms_have == roms_total,
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
