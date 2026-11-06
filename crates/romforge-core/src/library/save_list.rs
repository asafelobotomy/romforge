//! Milestone 27, stage 2: the saves in `Saves/`, each with its game when
//! the evidence points at exactly one (`saves::index`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use rusqlite::Connection;

use crate::dat::serial_key;
use crate::saves::index::{self, Known, Save};
use crate::{Error, Library, systems};

impl Library {
    /// Every save in the RomForge folder's `Saves/`, matched to games.
    /// Read-only; a library with no RomForge folder has none.
    pub fn list_saves(&self) -> Result<Vec<Save>, Error> {
        let Some(home) = &self.home else {
            return Ok(Vec::new());
        };
        let mut saves = index::walk(&home.saves());
        let known = known(&self.conn, &saves)?;
        index::match_all(&mut saves, &known);
        Ok(saves)
    }
}

/// The serials and file names the saves' systems have.
fn known(conn: &Connection, saves: &[Save]) -> Result<Known, Error> {
    let mut known = Known::default();
    let keys: BTreeSet<&str> = saves.iter().map(|s| s.system.as_str()).collect();
    for key in keys {
        let Some(system) = systems::by_key(key) else {
            continue;
        };
        let mut stmt = conn.prepare("SELECT title, serial FROM serials WHERE system_id = ?1")?;
        for row in stmt.query_map([system.id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (title, serial) = row?;
            known
                .serials
                .entry((key.to_owned(), serial_key(&serial)))
                .or_default()
                .insert(title);
        }
        add_stems(conn, key, system.id, &mut known.stems)?;
    }
    Ok(known)
}

/// The files of games on a system, matched by hash or named by you, by
/// the stem of their file name.
fn add_stems(
    conn: &Connection,
    key: &str,
    system_id: i64,
    stems: &mut BTreeMap<(String, String), BTreeSet<String>>,
) -> Result<(), Error> {
    let mut stmt = conn.prepare(
        "SELECT f.path, g.name FROM files f
           JOIN matches m ON m.file_id = f.file_id JOIN roms r ON r.rom_id = m.rom_id
           JOIN games g ON g.game_id = r.game_id JOIN dats d ON d.dat_id = g.dat_id
          WHERE d.system_id = ?1
         UNION
         SELECT f.path, mi.title FROM files f
           JOIN manual_ids mi ON mi.sha1 = f.sha1 AND mi.size = f.size
          WHERE mi.system_id = ?1",
    )?;
    for row in stmt.query_map([system_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (path, title) = row?;
        if let Some(stem) = PathBuf::from(path).file_stem() {
            stems
                .entry((key.to_owned(), stem.to_string_lossy().into_owned()))
                .or_default()
                .insert(title);
        }
    }
    Ok(())
}
