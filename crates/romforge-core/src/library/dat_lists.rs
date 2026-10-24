//! Serial and clone lists, and how imported DATs compare upstream.

use rusqlite::{OptionalExtension, params};

use crate::{Error, Library, dat, fetch, organize, systems};

/// How an imported DAT compares with the latest one upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatState {
    Current,
    Outdated,
    Missing,
    /// Imported from libretro's No-Intro mirror; update with `dat download`.
    Mirror,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DatStatus {
    pub name: String,
    pub system: &'static str,
    pub latest: String,
    pub dat_id: Option<i64>,
    pub imported: Option<String>,
    pub state: DatState,
}

impl Library {
    /// Store a libretro serial list (`fetch::Source::Serials`) for
    /// `system_key`: each record under its own title, and under the name of
    /// every DAT game of the system with one of its CRC32s (No-Intro may
    /// have renamed it since). Returns how many serials were stored. A
    /// system libretro has no list for stores none.
    pub fn download_serials(&mut self, system_key: &str, source: fetch::Source) -> Result<usize, Error> {
        let Some(url) = source.url() else {
            return Ok(0);
        };
        let Some(body) = fetch::get_optional(&url)? else {
            return Ok(0);
        };
        let tag = match source {
            fetch::Source::SerialDat(_) => "libretro-dat",
            _ => "libretro",
        };
        self.store_serials_from(system_key, &body, tag)
    }

    /// DAT titles of `system_key` with `serial` (compared by
    /// [`dat::serial_key`]). Several discs or versions can share a serial,
    /// so this narrows, never identifies (house rule 8).
    pub fn titles_by_serial(&self, system_key: &str, serial: &str) -> Result<Vec<String>, Error> {
        let system =
            systems::by_key(system_key).ok_or_else(|| Error::UnknownSystem(system_key.to_owned()))?;
        Ok(self
            .conn
            .prepare("SELECT DISTINCT title FROM serials WHERE system_id = ?1 AND key = ?2 ORDER BY title")?
            .query_map(params![system.id, dat::serial_key(serial)], |r| r.get(0))?
            .collect::<Result<_, _>>()?)
    }

    /// Store a Retool clone list (JSON) named `list` for `system_key`,
    /// replacing what an earlier version of that list stored. A title
    /// already in another group (of this list or another) keeps that one.
    /// Returns how many titles were stored.
    pub fn store_clone_list(&mut self, system_key: &str, list: &str, json: &[u8]) -> Result<usize, Error> {
        #[derive(serde::Deserialize)]
        struct List {
            variants: Vec<Variant>,
        }
        #[derive(serde::Deserialize)]
        struct Variant {
            group: String,
            /// None in a compilation-only entry (a multi-game cartridge),
            /// which isn't one game under other names.
            #[serde(default)]
            titles: Vec<Title>,
        }
        #[derive(serde::Deserialize)]
        struct Title {
            #[serde(rename = "searchTerm")]
            search_term: String,
            #[serde(default)]
            priority: Option<i64>,
            /// Regex and other special matching: skipped.
            #[serde(default, rename = "nameType")]
            name_type: Option<String>,
        }
        let system =
            systems::by_key(system_key).ok_or_else(|| Error::UnknownSystem(system_key.to_owned()))?;
        let parsed: List =
            serde_json::from_slice(json).map_err(|e| Error::Dat(format!("clone list: {e}")))?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM clone_groups WHERE system_id = ?1 AND list = ?2",
            params![system.id, list],
        )?;
        let mut n = 0;
        {
            let mut put = tx.prepare(
                "INSERT OR IGNORE INTO clone_groups (system_id, title, grp, priority, list)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for v in &parsed.variants {
                for t in v.titles.iter().filter(|t| t.name_type.is_none()) {
                    n += put.execute(params![
                        system.id,
                        organize::base_title(&t.search_term),
                        v.group,
                        t.priority.unwrap_or(1),
                        list
                    ])?;
                }
            }
        }
        tx.commit()?;
        Ok(n)
    }

    /// [`Library::download_serials`] with libretro's `metadat/serial` list
    /// already in hand.
    pub fn store_serials(&mut self, system_key: &str, list: &[u8]) -> Result<usize, Error> {
        self.store_serials_from(system_key, list, "libretro")
    }

    /// Store a serial list under `source`, replacing that source's earlier
    /// list for the system (each list replaces only its own).
    pub fn store_serials_from(
        &mut self,
        system_key: &str,
        list: &[u8],
        source: &str,
    ) -> Result<usize, Error> {
        let system =
            systems::by_key(system_key).ok_or_else(|| Error::UnknownSystem(system_key.to_owned()))?;
        let records = dat::parse_cmp_records(&String::from_utf8_lossy(list))?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM serials WHERE system_id = ?1 AND source = ?2",
            params![system.id, source],
        )?;
        let mut stored = 0;
        {
            let mut put = tx.prepare(
                "INSERT OR IGNORE INTO serials (system_id, title, serial, key, source)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            let mut by_crc = tx.prepare(
                "SELECT DISTINCT g.name FROM roms r JOIN games g ON g.game_id = r.game_id
                 JOIN dats d ON d.dat_id = g.dat_id WHERE d.system_id = ?1 AND r.crc32 = ?2",
            )?;
            for rec in &records {
                let serials: Vec<String> = rec
                    .fields
                    .iter()
                    .filter(|(k, _)| k == "serial")
                    .flat_map(|(_, v)| dat::split_serials(v))
                    .chain(rec.serials.iter().flat_map(|v| dat::split_serials(v)))
                    .collect();
                let mut titles: Vec<String> = rec.title.iter().cloned().collect();
                for crc in &rec.crcs {
                    for name in by_crc.query_map(params![system.id, crc], |r| r.get::<_, String>(0))? {
                        titles.push(name?);
                    }
                }
                titles.sort();
                titles.dedup();
                for title in &titles {
                    for serial in &serials {
                        stored +=
                            put.execute(params![system.id, title, serial, dat::serial_key(serial), source])?;
                    }
                }
            }
        }
        tx.commit()?;
        Ok(stored)
    }

    /// Compare imported DATs with No-Intro's `profile.xml` (from
    /// [`fetch::nointro_profile`]). Lists every profile entry for a known
    /// system. A system's main DAT counts as [`DatState::Mirror`] when it
    /// came from libretro's mirror, whose names and version format differ.
    pub fn nointro_status(&self, profile: &[fetch::ProfileEntry]) -> Result<Vec<DatStatus>, Error> {
        let mut by_name = self.conn.prepare(
            "SELECT dat_id, version, source_path FROM dats WHERE name = ?1
                 ORDER BY source_path LIKE 'http%', dat_id DESC LIMIT 1",
        )?;
        let mut mirror_of = self.conn.prepare(
            "SELECT dat_id, version, source_path FROM dats WHERE system_id = ?1 AND source_path LIKE ?2
             ORDER BY dat_id LIMIT 1",
        )?;
        let mirror_like = format!("{}%", fetch::LIBRETRO_NOINTRO_BASE);
        let mut out = Vec::new();
        for entry in profile {
            let Some(system) = systems::from_dat_name(&entry.name) else {
                continue;
            };
            let row = |r: &rusqlite::Row| -> rusqlite::Result<(i64, Option<String>, String)> {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            };
            let mut imported = by_name.query_row([&entry.name], row).optional()?;
            let main_set = !entry.name.ends_with(')');
            if imported.is_none() && main_set {
                imported = mirror_of
                    .query_row(params![system.id, mirror_like], row)
                    .optional()?;
            }
            let state = match &imported {
                None => DatState::Missing,
                Some((_, _, src)) if fetch::Source::is_mirror_url(src) => DatState::Mirror,
                Some((_, v, _)) if v.as_deref() == Some(entry.version.as_str()) => DatState::Current,
                Some(_) => DatState::Outdated,
            };
            out.push(DatStatus {
                name: entry.name.clone(),
                system: system.key,
                latest: entry.version.clone(),
                dat_id: imported.as_ref().map(|i| i.0),
                imported: imported.and_then(|i| i.1),
                state,
            });
        }
        Ok(out)
    }
}
