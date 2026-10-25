//! Milestone 20, step 6: RetroBIOS's system files kept by size and hash
//! (`firmware_hashes`), so the library can tell a scanned file that is a
//! BIOS file. Only files with a size and a SHA1 or MD5 (house rule 8), for
//! romforge's platforms other than arcade (RetroBIOS's arcade sets overlap
//! MAME's DATs), and no BIOS zips. Built again when the list changes.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use rusqlite::{Connection, params};

use super::{Profile, load};
use crate::{Error, systems};

const BUILT_FROM: &str = "firmware_index";

/// The romforge platform of RetroBIOS's `id`.
fn platform(id: &str) -> Option<&'static systems::System> {
    systems::SYSTEMS
        .iter()
        .find(|s| !s.arcade && s.retrobios.contains(&id))
}

/// (size, sha1 or "", md5 or "", system id, name) for every file.
fn rows(profiles: &[Profile]) -> BTreeSet<(u64, String, String, i64, String)> {
    let mut out = BTreeSet::new();
    for p in profiles {
        for f in &p.files {
            let Some(size) = f.size else { continue };
            if f.archive || !f.contents.is_empty() {
                continue;
            }
            let ids: Vec<&str> = match &f.system {
                Some(s) => vec![s.as_str()],
                None => p.systems.iter().map(String::as_str).collect(),
            };
            for sys in ids.into_iter().filter_map(platform) {
                for h in &f.sha1 {
                    out.insert((size, h.clone(), String::new(), sys.id, f.name.clone()));
                }
                for h in &f.md5 {
                    out.insert((size, String::new(), h.clone(), sys.id, f.name.clone()));
                }
            }
        }
    }
    out
}

/// Build the index from the list in `dir` unless it was built from this
/// one already. Returns whether it was built.
pub(crate) fn index(conn: &Connection, dir: &Path) -> Result<bool, Error> {
    let (profiles, generated_at) = load(dir)?;
    let built: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [BUILT_FROM], |r| {
            r.get(0)
        })
        .ok();
    if built.as_deref() == Some(generated_at.as_str()) {
        return Ok(false);
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM firmware_hashes", [])?;
    {
        let mut put = tx.prepare(
            "INSERT INTO firmware_hashes (size, sha1, md5, system_id, name)
             VALUES (?1, NULLIF(?2, ''), NULLIF(?3, ''), ?4, ?5)",
        )?;
        for (size, sha1, md5, system, name) in rows(&profiles) {
            put.execute(params![size, sha1, md5, system, name])?;
        }
    }
    tx.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![BUILT_FROM, generated_at],
    )?;
    tx.commit()?;
    Ok(true)
}

/// RetroBIOS's system files by (size, hash), read once for the library view.
pub(crate) struct BiosIndex {
    by_sha1: HashMap<(u64, String), (i64, String)>,
    by_md5: HashMap<(u64, String), (i64, String)>,
}

impl BiosIndex {
    pub(crate) fn read(conn: &Connection) -> Result<BiosIndex, Error> {
        let mut out = BiosIndex {
            by_sha1: HashMap::new(),
            by_md5: HashMap::new(),
        };
        let mut stmt = conn.prepare("SELECT size, sha1, md5, system_id, name FROM firmware_hashes")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, u64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        for row in rows {
            let (size, sha1, md5, system, name) = row?;
            if let Some(h) = sha1 {
                out.by_sha1.entry((size, h)).or_insert((system, name.clone()));
            }
            if let Some(h) = md5 {
                out.by_md5.entry((size, h)).or_insert((system, name));
            }
        }
        Ok(out)
    }

    /// A scanned file that is a BIOS file: (system id, its name), by size
    /// and SHA1 or MD5.
    pub(crate) fn get(&self, size: u64, sha1: &str, md5: &str) -> Option<(i64, String)> {
        self.by_sha1
            .get(&(size, sha1.to_ascii_lowercase()))
            .or_else(|| self.by_md5.get(&(size, md5.to_ascii_lowercase())))
            .cloned()
    }
}
