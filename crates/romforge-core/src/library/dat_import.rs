//! Storing one parsed DAT.

use std::path::Path;

use rusqlite::{Connection, params};
use sha2::{Digest as _, Sha256};

use crate::{Error, ImportedDat, Library, dat, fetch, hash, identify, patcher, systems};

impl Library {
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
        // Without a system either, it is named after its file, so two
        // different nameless DATs don't replace each other.
        if dat.name.is_empty() {
            dat.name = source_stem(source);
        }

        let defer = self.batch_depth > 0;
        // Immediate: this reads, then writes (see `scan_with`).
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        // The same file again is a no-op; a new version of a DAT (same name,
        // and same source family: see `fetch::source_family`) replaces the
        // old one, and folders scoped to it follow along. No-Intro and Redump both name a DAT
        // "Sony - PlayStation Portable", so a download never replaces a
        // same-named DAT from elsewhere.
        let existing: Option<(i64, String, Option<i64>)> = {
            let mut stmt = tx.prepare(
                "SELECT dat_id, source_sha256, source_path, system_id FROM dats
                 WHERE source_sha256 = ?2 OR name = ?1
                 ORDER BY source_sha256 = ?2 DESC, dat_id",
            )?;
            let rows = stmt.query_map(params![dat.name, sha256], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })?;
            let family = fetch::source_family(source);
            let mut found = None;
            for row in rows {
                let (id, sha, path, system_id) = row?;
                if sha == sha256 || fetch::source_family(&path) == family {
                    found = Some((id, sha, system_id));
                    break;
                }
            }
            found
        };
        // The same file for another system (given by hand) is imported
        // again for it, replacing the old one, so its folders and serials
        // follow.
        if let Some((old_id, old_sha, old_system)) = &existing
            && *old_sha == sha256
            && (system_key.is_none() || system.map(|s| s.id) == *old_system)
        {
            return Ok(ImportedDat {
                dat_id: *old_id,
                name: dat.name,
                system: old_system.and_then(systems::by_id).map(|s| s.key),
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
        let replaced = existing.map(|(id, _, _)| id);
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
            // Kept with the DAT, so it survives a failed batch or a crash.
            super::open::mark_needs_identify(&tx)?;
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
}

/// The file name without its extension of a DAT's `source` (a path,
/// `zip#member` or URL): `TOSEC/x.dat` of `a.zip#TOSEC/x.dat` gives `x`.
fn source_stem(source: &str) -> String {
    let name = source.rsplit('#').next().unwrap_or(source);
    let name = name.split('?').next().unwrap_or(name);
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_owned())
}

/// DAT serials whose game no DAT of its system has any more (the DAT was
/// replaced or removed). libretro's serials stay until its list is stored
/// again.
pub(super) fn forget_stale_serials(conn: &Connection) -> Result<(), Error> {
    conn.execute(
        "DELETE FROM serials WHERE source = 'dat' AND NOT EXISTS (
           SELECT 1 FROM games g JOIN dats d ON d.dat_id = g.dat_id
           WHERE g.name = serials.title AND d.system_id = serials.system_id)",
        [],
    )?;
    Ok(())
}
