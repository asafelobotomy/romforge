//! What is known about a row's files, and the DAT games it points at.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension, params};

use super::{Facts, Proof};
use crate::ident::GameId;
use crate::{Error, dat, systems, view};

/// Your hash-verified copy of a DAT game's (single) ROM, if any.
pub(crate) fn verified_copy(
    conn: &Connection,
    system_id: i64,
    title: &str,
) -> Result<Option<(String, String)>, Error> {
    Ok(conn
        .prepare_cached(concat!(
            "SELECT f.path, f.member FROM files f JOIN matches m ON m.file_id = f.file_id
             JOIN roms r ON r.rom_id = m.rom_id JOIN games g ON g.game_id = r.game_id
             JOIN dats d ON d.dat_id = g.dat_id
             WHERE d.system_id = ?1 AND g.name = ?2 AND m.via IN ",
            crate::report::strong!(),
            " AND m.stripped = 0
               AND f.status IN ",
            crate::report::holding!(),
            "
               AND (SELECT count(*) FROM roms x WHERE x.game_id = g.game_id) = 1
             LIMIT 1"
        ))?
        .query_row(params![system_id, title], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?)
}

/// Whether `patch` applied to your verified copy of the game gives exactly
/// the file with `sha1`.
pub(super) fn applies(
    conn: &Connection,
    patch: &[u8],
    system_id: i64,
    title: &str,
    sha1: &str,
) -> Result<bool, Error> {
    let Some((path, member)) = verified_copy(conn, system_id, title)? else {
        return Ok(false);
    };
    let Some(base) = crate::patch::read_entry(std::path::Path::new(&path), &member) else {
        return Ok(false);
    };
    let Some(out) = crate::patch::apply(patch, &base) else {
        return Ok(false);
    };
    Ok(crate::hash::hash_stream(&out[..], out.len() as u64)?.raw.sha1 == sha1)
}

pub(super) fn facts(
    conn: &Connection,
    d: &view::GameDetail,
    text: Option<&str>,
    system: Option<&str>,
    patches: &mut crate::patch::PatchFinder,
) -> Result<Facts, Error> {
    let own = crate::search::default_text(d);
    let text = text
        .filter(|t| !t.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| own.clone());
    // A compressed disc image counts by the plain image it holds.
    let sizes: Vec<u64> = d
        .files
        .iter()
        .map(|f| f.game_id.as_ref().and_then(|i| i.image_size).unwrap_or(f.size))
        .collect();
    let ids: Vec<GameId> = d.files.iter().filter_map(|f| f.game_id.clone()).collect();
    let mut systems: Vec<i64> = match system.or(d.row.system.as_deref()) {
        Some(key) => systems::by_key(key).map(|s| vec![s.id]).unwrap_or_default(),
        None => Vec::new(),
    };
    if systems.is_empty() {
        // The header's kind first, then systems using the extension.
        systems.extend(ids.iter().flat_map(|i| systems::by_ident(i.kind)).map(|s| s.id));
        let ext = d.files.first().and_then(|f| {
            std::path::Path::new(f.path.rsplit('#').next().unwrap_or(&f.path))
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
        });
        if let Some(ext) = ext {
            systems.extend(
                systems::SYSTEMS
                    .iter()
                    .filter(|s| s.extensions.contains(&ext.as_str()))
                    .map(|s| s.id),
            );
        }
        let mut seen = BTreeSet::new();
        systems.retain(|s| seen.insert(*s));
    }
    let mut crc = BTreeSet::new();
    let mut stmt = conn.prepare_cached(
        "SELECT d.system_id, g.name FROM matches m JOIN roms r ON r.rom_id = m.rom_id
         JOIN games g ON g.game_id = r.game_id JOIN dats d ON d.dat_id = g.dat_id
         WHERE m.file_id = ?1 AND m.via = 'crc32' AND d.system_id IS NOT NULL",
    )?;
    for id in d.row.file_ids() {
        for r in stmt.query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
            crc.insert(r?);
        }
    }
    // Patches beside the files ("Game.sfc" and "Game.bps"), on disk or in
    // the same archive.
    let mut patched = BTreeMap::new();
    let mut unnamed = Vec::new();
    let mut file = conn.prepare_cached("SELECT path, member, crc32, sha1 FROM files WHERE file_id = ?1")?;
    let mut roms = conn.prepare_cached(
        "SELECT DISTINCT d.system_id, g.name FROM roms r JOIN games g ON g.game_id = r.game_id
         JOIN dats d ON d.dat_id = g.dat_id
         WHERE r.crc32 = ?1 AND r.size = ?2 AND d.system_id IS NOT NULL",
    )?;
    for id in d.row.file_ids() {
        let (path, member, crc, sha1): (String, String, String, String) =
            file.query_row([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        for data in patches.beside(std::path::Path::new(&path), &member) {
            let Some(info) = crate::patch::info(&data) else {
                continue;
            };
            let Some((size, source_crc)) = info.source else {
                unnamed.push((data, sha1.clone()));
                continue;
            };
            let makes_it = info.target.is_some_and(|(_, t)| format!("{t:08x}") == crc);
            let games: Vec<(i64, String)> = roms
                .query_map(params![format!("{source_crc:08x}"), size], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?
                .collect::<Result<_, _>>()?;
            for (sid, title) in games {
                let proof = if makes_it && applies(conn, &data, sid, &title, &sha1)? {
                    Proof::Applied
                } else if makes_it {
                    Proof::Turns
                } else {
                    Proof::MadeFrom
                };
                let e = patched.entry((sid, title)).or_insert(proof);
                *e = (*e).max(proof);
            }
        }
    }
    // NKit: the original image's CRC32, byte order undocumented, so both.
    let mut nkit = BTreeSet::new();
    let mut by_crc = conn.prepare_cached(
        "SELECT DISTINCT d.system_id, g.name FROM roms r JOIN games g ON g.game_id = r.game_id
         JOIN dats d ON d.dat_id = g.dat_id WHERE r.crc32 IN (?1, ?2) AND d.system_id IS NOT NULL",
    )?;
    for c in ids.iter().filter_map(|i| i.source_crc) {
        let (be, le) = (format!("{c:08x}"), format!("{:08x}", c.swap_bytes()));
        for r in by_crc.query_map(params![be, le], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })? {
            let r = r?;
            if systems.is_empty() || systems.contains(&r.0) {
                nkit.insert(r);
            }
        }
    }
    Ok(Facts {
        text,
        own,
        sizes,
        ids,
        systems,
        crc,
        patched,
        unnamed,
        nkit,
    })
}

/// DAT games whose serial carries one of the file's IDs, with that serial.
pub(super) fn by_serial(conn: &Connection, f: &Facts) -> Result<BTreeMap<(i64, String), String>, Error> {
    let mut out = BTreeMap::new();
    let mut stmt = conn.prepare_cached(
        "SELECT system_id, title, serial FROM serials WHERE system_id = ?1
           AND (key = ?2 OR (length(?2) = 4 AND instr(key, ?2) > 0))",
    )?;
    for id in &f.ids {
        let Some(serial) = &id.serial else { continue };
        let mut key = dat::serial_key(serial);
        // GameCube / Wii / Wii U: "GALE01" is game code GALE and maker 01;
        // Redump's serials ("DL-DOL-GALE-USA") and the Wii U list carry the
        // game code only.
        let with_maker = matches!(id.kind, "gc" | "wii" | "wiiu");
        if with_maker {
            key.truncate(4);
        }
        if key.len() < 4 {
            continue;
        }
        for s in systems::by_ident(id.kind).filter(|s| f.systems.contains(&s.id)) {
            for r in stmt.query_map(params![s.id, key], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })? {
                let (sid, title, serial) = r?;
                // A short game code must be a whole part of the serial
                // ("AQ3J" in SHVC-AQ3J-JPN), not letters across two parts.
                // A game code with its maker ("AGMP01", in the Wii U list)
                // is the same game code.
                let stored = dat::serial_key(&serial);
                let maker_of = with_maker && stored.len() == 6 && stored.starts_with(&key);
                if stored != key && !code_in(&key, &serial) && !maker_of {
                    continue;
                }
                out.entry((sid, title)).or_insert(serial);
            }
        }
    }
    Ok(out)
}

/// Whether `code` (letters and digits) is one of `serial`'s parts, split at
/// anything else ("SHVC-AQ3J-JPN" has AQ3J).
pub(super) fn code_in(code: &str, serial: &str) -> bool {
    serial
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case(code))
}

/// The DAT names of `systems`, each with its systems.
pub(super) fn dat_names(conn: &Connection, systems: &[i64]) -> Result<BTreeMap<String, Vec<i64>>, Error> {
    let mut out: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    let ids = serde_json::to_string(systems).unwrap_or_else(|_| "[]".into());
    let mut stmt = conn.prepare(
        "SELECT DISTINCT d.system_id, g.name FROM games g JOIN dats d ON d.dat_id = g.dat_id
         WHERE d.system_id IS NOT NULL AND (?1 = 0 OR d.system_id IN (SELECT value FROM json_each(?2)))",
    )?;
    for r in stmt.query_map(params![systems.len() as i64, ids], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })? {
        let (sid, name) = r?;
        out.entry(name).or_default().push(sid);
    }
    Ok(out)
}
