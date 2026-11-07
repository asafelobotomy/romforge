//! The library's files as a list, with the games and systems each is for.

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::Serialize;

use super::{RELEVANT_MATCH, status};
use crate::Error;
use crate::identify::Status;

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

/// `path` for a plain file, `path#member` for an archive member.
pub(crate) fn display_path(path: &str, member: &str) -> String {
    if member.is_empty() {
        path.to_owned()
    } else {
        format!("{path}#{member}")
    }
}

/// A file's place, where its game's other files are: the archive it is in,
/// or the folder holding it.
pub(crate) fn place_of<'a>(path: &'a str, member: &str) -> Option<&'a str> {
    if member.is_empty() {
        path.rsplit_once('/').map(|(dir, _)| dir)
    } else {
        Some(path)
    }
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
