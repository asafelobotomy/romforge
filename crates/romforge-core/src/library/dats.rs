//! Importing and downloading DATs.

use std::io::{BufReader, Read};
use std::path::Path;

use rusqlite::{Connection, params};

use super::dat_import::forget_stale_serials;
use crate::{Error, Library, fetch, systems};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ImportedDat {
    pub dat_id: i64,
    pub name: String,
    pub system: Option<&'static str>,
    pub header_hint: Option<String>,
    pub games: usize,
    pub roms: usize,
    /// The older DAT with the same name that this import replaced.
    pub replaced: Option<i64>,
    /// The exact same file was already imported; nothing changed.
    pub unchanged: bool,
}

fn read_file(path: &Path) -> Result<Vec<u8>, Error> {
    let mut buf = Vec::new();
    BufReader::new(std::fs::File::open(path)?).read_to_end(&mut buf)?;
    Ok(buf)
}

impl Library {
    /// Import a DAT (Logiqx XML or clrmamepro text). The system is taken from `system_key` when
    /// given, else guessed from the DAT name; if neither works the DAT has no
    /// system (it still matches when a folder is scanned without a scope).
    pub fn import_dat(
        &mut self,
        path: impl AsRef<Path>,
        system_key: Option<&str>,
    ) -> Result<ImportedDat, Error> {
        let path = std::path::absolute(path.as_ref())?;
        let bytes = read_file(&path)?;
        self.import_dat_bytes(&bytes, &path.to_string_lossy(), system_key)
    }

    /// Import a DAT file, or every DAT inside a `.zip` of DATs (as No-Intro
    /// and Redump hand them out). `system_key` is refused for a zip holding
    /// more than one DAT, since they are for different systems.
    pub fn import_dats(
        &mut self,
        path: impl AsRef<Path>,
        system_key: Option<&str>,
    ) -> Result<Vec<ImportedDat>, Error> {
        let path = std::path::absolute(path.as_ref())?;
        let dats = fetch::dats_in(&read_file(&path)?)?;
        if system_key.is_some() && dats.len() > 1 {
            return Err(Error::Dat(format!(
                "{} holds {} DATs; --system applies to one",
                path.display(),
                dats.len()
            )));
        }
        self.batch(|lib| {
            dats.into_iter()
                .map(|fetch::DatBytes { member, bytes }| {
                    let source = match member {
                        Some(m) => format!("{}#{m}", path.display()),
                        None => path.to_string_lossy().into_owned(),
                    };
                    lib.import_dat_bytes(&bytes, &source, system_key)
                })
                .collect()
        })
    }

    /// Download a source's DATs and import them for `system_key`: one DAT,
    /// or every DAT of a TOSEC system. Downloading the same DAT again changes
    /// nothing; a newer one replaces the old. An empty result means the
    /// TOSEC release is already imported for this system, so the pack was not
    /// fetched (or, for `Serials`, that the list was stored).
    pub fn download_dat(
        &mut self,
        system_key: &str,
        source: fetch::Source,
    ) -> Result<Vec<ImportedDat>, Error> {
        if let fetch::Source::Serials(_) | fetch::Source::SerialDat(_) = source {
            self.download_serials(system_key, source)?;
            return Ok(Vec::new());
        }
        if let fetch::Source::CloneList(..) = source {
            if let (Some(url), fetch::Source::CloneList(name, group)) = (source.url(), source)
                && let Some(body) = fetch::get_optional(&url)?
            {
                self.store_clone_list(system_key, &format!("{name} ({group})"), &body)?;
            }
            return Ok(Vec::new());
        }
        if let fetch::Source::Tosec(name) = source {
            let url = fetch::tosec_pack_url()?;
            let (prefix, part) = tosec_paths(&url, name);
            // Done when this release is in and no older one is left (an
            // older DAT left over means the last run did not finish).
            let (have, older): (bool, bool) = self.conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM dats WHERE substr(source_path, 1, length(?1)) = ?1),
                        EXISTS (SELECT 1 FROM dats WHERE substr(source_path, 1, length(?2)) = ?2
                                  AND instr(source_path, ?3) > 0
                                  AND substr(source_path, 1, length(?1)) <> ?1)",
                params![prefix, fetch::TOSEC_BASE, part],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if have && !older {
                return Ok(Vec::new());
            }
            let pack = fetch::tosec_pack(&url)?;
            let dats = fetch::tosec_dats(&pack, name)?;
            if dats.is_empty() {
                return Err(Error::Dat(format!("the TOSEC pack has no {name} DATs")));
            }
            // Every DAT is tried even if one fails: the "already imported"
            // check above would otherwise never come back for the rest.
            return self.batch(|lib| {
                let total = dats.len();
                let mut imported = Vec::new();
                let mut failed = Vec::new();
                for d in dats {
                    let member = d.member.unwrap_or_default();
                    let source = format!("{url}#{member}");
                    match lib.import_dat_bytes(&d.bytes, &source, Some(system_key)) {
                        Ok(i) => imported.push(i),
                        Err(e) => failed.push(format!("{member}: {e}")),
                    }
                }
                if failed.is_empty() {
                    if forget_older_tosec(&mut lib.conn, &url, name)? > 0 {
                        lib.batch_dirty = true;
                    }
                    Ok(imported)
                } else {
                    Err(Error::Dat(format!(
                        "{} of {total} TOSEC {name} DATs failed ({} imported): {}",
                        failed.len(),
                        imported.len(),
                        failed.join("; ")
                    )))
                }
            });
        }
        let url = source.url().unwrap_or_default();
        let body = fetch_one(&url)?;
        self.import_one(system_key, &url, &body)
    }

    /// Import a one-DAT source's download ([`fetch_one`]).
    pub(super) fn import_one(
        &mut self,
        system_key: &str,
        url: &str,
        body: &[u8],
    ) -> Result<Vec<ImportedDat>, Error> {
        let mut dats = fetch::dats_in(body)?;
        if dats.len() != 1 {
            return Err(Error::Dat(format!(
                "{url}: expected one DAT, found {}",
                dats.len()
            )));
        }
        let dat = dats.remove(0);
        Ok(vec![self.import_dat_bytes(&dat.bytes, url, Some(system_key))?])
    }

    /// Download and import every DAT of a system ([`fetch::sources`]; hacks
    /// and homebrew with `extras`), reporting each source as it finishes;
    /// one failing does not stop the rest.
    pub fn download_dats(
        &mut self,
        system_key: &str,
        extras: bool,
        mut each: impl FnMut(fetch::Source, Result<Vec<ImportedDat>, Error>),
    ) -> Result<(), Error> {
        let system =
            systems::by_key(system_key).ok_or_else(|| Error::UnknownSystem(system_key.to_owned()))?;
        let sources = fetch::sources(system, extras);
        if sources.is_empty() {
            return Err(Error::NoDownload(system.key));
        }
        self.batch(|lib| {
            for source in sources {
                let result = lib.download_dat(system.key, source);
                each(source, result);
            }
            Ok(())
        })
    }
}

/// A source that is one DAT file at its URL: downloaded on its own (on any
/// thread), then imported ([`Library::import_one`]).
pub(super) fn is_one_file(source: fetch::Source) -> bool {
    use fetch::Source::*;
    matches!(
        source,
        NoIntro(_) | Redump(_) | Hacks(_) | Homebrew(_) | Libretro(_) | Url(_)
    )
}

/// Download a one-DAT source (no database), refusing a web page.
pub(super) fn fetch_one(url: &str) -> Result<Vec<u8>, Error> {
    let body = fetch::get(url)?;
    let head = String::from_utf8_lossy(&body[..body.len().min(512)]).to_ascii_lowercase();
    if head.contains("<html") || head.contains("<!doctype html") {
        return Err(Error::Http(format!("{url}: got a web page, not a DAT")));
    }
    Ok(body)
}

mod all;

/// The source paths of a TOSEC release's DATs for `name`, and the part
/// every release's paths for it share.
fn tosec_paths(url: &str, name: &str) -> (String, String) {
    (format!("{url}#TOSEC/{name} - "), format!("#TOSEC/{name} - "))
}

/// Remove `name`'s DATs from TOSEC releases other than `url`'s: once a
/// release is fully in, what it dropped or renamed goes too. A folder
/// scoped to one of those DATs follows the new release's DAT of the same
/// name, else keeps that DAT's system as its scope (never widening to
/// all DATs). All in one transaction, with identify marked as due.
fn forget_older_tosec(conn: &mut Connection, url: &str, name: &str) -> Result<usize, Error> {
    let (prefix, part) = tosec_paths(url, name);
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let old: Vec<(i64, String, Option<i64>)> = tx
        .prepare(
            "SELECT dat_id, name, system_id FROM dats WHERE substr(source_path, 1, length(?2)) = ?2
               AND instr(source_path, ?3) > 0 AND substr(source_path, 1, length(?1)) <> ?1",
        )?
        .query_map(params![prefix, fetch::TOSEC_BASE, part], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<Result<_, _>>()?;
    for (old_id, old_name, old_system) in &old {
        let new_id: Option<i64> = tx.query_row(
            "SELECT max(dat_id) FROM dats WHERE name = ?1 AND substr(source_path, 1, length(?2)) = ?2",
            params![old_name, prefix],
            |r| r.get(0),
        )?;
        match new_id {
            Some(new_id) => tx.execute(
                "UPDATE scan_roots SET dat_id = ?1 WHERE dat_id = ?2",
                [new_id, *old_id],
            )?,
            None => tx.execute(
                "UPDATE scan_roots SET dat_id = NULL, system_id = coalesce(system_id, ?2)
                 WHERE dat_id = ?1",
                params![old_id, old_system],
            )?,
        };
        tx.execute("DELETE FROM dats WHERE dat_id = ?1", [old_id])?;
    }
    if !old.is_empty() {
        forget_stale_serials(&tx)?;
        super::open::mark_needs_identify(&tx)?;
    }
    tx.commit()?;
    Ok(old.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dat(name: &str, games: &[(&str, &str)]) -> String {
        let games: String = games
            .iter()
            .map(|(g, serial)| {
                format!(r#"<game name="{g}"><serial>{serial}</serial><rom name="{g}.gb" size="1" crc="0000000{}"/></game>"#, g.len() % 10)
            })
            .collect();
        format!(r#"<datafile><header><name>{name}</name></header>{games}</datafile>"#)
    }

    fn serials(lib: &Library) -> Vec<(String, String)> {
        lib.conn
            .prepare("SELECT title, serial FROM serials ORDER BY title")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// A replaced DAT's serials go with it; a game's new serial replaces
    /// its old one.
    #[test]
    fn serials_follow_their_dat() {
        let mut lib = Library::open_in_memory().unwrap();
        let v1 = dat(
            "Nintendo - Game Boy",
            &[("Gone", "DMG-AAAA"), ("Kept", "DMG-BBBB")],
        );
        let v2 = dat(
            "Nintendo - Game Boy",
            &[("Kept", "DMG-CCCC"), ("New", "DMG-DDDD")],
        );
        lib.import_dat_bytes(v1.as_bytes(), "https://example.org/gb.dat", None)
            .unwrap();
        lib.import_dat_bytes(v2.as_bytes(), "https://example.org/gb.dat", None)
            .unwrap();
        assert_eq!(
            serials(&lib),
            [
                ("Kept".into(), "DMG-CCCC".into()),
                ("New".into(), "DMG-DDDD".into())
            ]
        );
    }

    /// Once a TOSEC release is in, a DAT it no longer has goes (and its
    /// serials); another system's TOSEC DATs stay.
    #[test]
    fn older_tosec_dats_go() {
        let mut lib = Library::open_in_memory().unwrap();
        let old = "https://www.tosecdev.org/downloads/category/58?download=1";
        let new = "https://www.tosecdev.org/downloads/category/59?download=2";
        let gb = |kind: &str| dat(&format!("Nintendo Game Boy - {kind}"), &[(kind, "DMG-EEEE")]);
        let demos = lib
            .import_dat_bytes(
                gb("Demos").as_bytes(),
                &format!("{old}#TOSEC/Nintendo Game Boy - Demos (TOSEC-v1).dat"),
                Some("Game Boy"),
            )
            .unwrap();
        // A folder scoped to the DAT that goes.
        lib.conn
            .execute(
                "INSERT INTO scan_roots (path, dat_id, scanned_at) VALUES ('/demos', ?1, 'now')",
                [demos.dat_id],
            )
            .unwrap();
        lib.import_dat_bytes(
            gb("Games").as_bytes(),
            &format!("{new}#TOSEC/Nintendo Game Boy - Games (TOSEC-v2).dat"),
            Some("Game Boy"),
        )
        .unwrap();
        let other = dat("Nintendo Game Boy Color - Games", &[("Color", "CGB-FFFF")]);
        lib.import_dat_bytes(
            other.as_bytes(),
            &format!("{old}#TOSEC/Nintendo Game Boy Color - Games (TOSEC-v1).dat"),
            Some("Game Boy Color"),
        )
        .unwrap();
        assert_eq!(
            forget_older_tosec(&mut lib.conn, new, "Nintendo Game Boy").unwrap(),
            1
        );
        let names: Vec<String> = lib.summary().unwrap().dats.into_iter().map(|d| d.name).collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(!names.iter().any(|n| n.ends_with("Demos")), "{names:?}");
        assert!(serials(&lib).iter().all(|(t, _)| t != "Demos"));
        // Its folder keeps the DAT's system as its scope, not all DATs.
        let scope: (Option<i64>, Option<i64>) = lib
            .conn
            .query_row(
                "SELECT system_id, dat_id FROM scan_roots WHERE path = '/demos'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(scope, (Some(systems::by_key("Game Boy").unwrap().id), None));
    }
}
