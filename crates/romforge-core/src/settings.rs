//! The owner's folders and how cleanup & rename treats them.
//!
//! - **Source**: where unprocessed ROMs land. Refresh scans it.
//! - **Library** (optional): where cleanup & rename (milestone 10) puts
//!   processed games, sorted by system; Refresh scans it too and Play uses
//!   what is there.
//! - **Keep originals**: cleanup works on copies in the **staging** folder
//!   and leaves the Source files where they are; without it the originals
//!   go to the trash once the processed file is verified (house rule 7).
//! - **Staging**: scratch space for cleanup, `<cache>/staging` by default.
//!
//! Stored in the database, so the CLI and the app share them.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::Error;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub source_dir: Option<PathBuf>,
    pub library_dir: Option<PathBuf>,
    pub keep_originals: bool,
    /// `None` means [`default_staging_dir`].
    pub staging_dir: Option<PathBuf>,
}

/// `<cache>/staging`, the cleanup scratch folder unless one is set.
pub fn default_staging_dir() -> PathBuf {
    crate::enrich::default_cache_dir().join("staging")
}

impl Settings {
    pub fn staging(&self) -> PathBuf {
        self.staging_dir.clone().unwrap_or_else(default_staging_dir)
    }

    /// Check the folders make sense together, and make them absolute.
    /// Source and Library must exist and must not contain one another (a
    /// Refresh of one would otherwise pick up the other's files twice, and
    /// cleanup could feed on its own output). Staging may not exist yet but
    /// must stay outside both.
    pub fn validated(&self) -> Result<Settings, Error> {
        let bad = |m: String| Err(Error::Settings(m));
        let existing = |label: &str, p: &Option<PathBuf>| -> Result<Option<PathBuf>, Error> {
            match p {
                None => Ok(None),
                Some(p) if p.as_os_str().is_empty() => Ok(None),
                Some(p) => {
                    let c = std::fs::canonicalize(p)
                        .map_err(|e| Error::Settings(format!("{label} folder {}: {e}", p.display())))?;
                    if !c.is_dir() {
                        return Err(Error::Settings(format!(
                            "{label} {} is not a folder",
                            c.display()
                        )));
                    }
                    Ok(Some(c))
                }
            }
        };
        let source = existing("Source", &self.source_dir)?;
        let library = existing("Library", &self.library_dir)?;
        let staging = match &self.staging_dir {
            Some(p) if !p.as_os_str().is_empty() => {
                Some(std::fs::canonicalize(p).or_else(|_| std::path::absolute(p))?)
            }
            _ => None,
        };
        let overlap = |a: &Path, b: &Path| a.starts_with(b) || b.starts_with(a);
        if let (Some(s), Some(l)) = (&source, &library)
            && overlap(s, l)
        {
            return bad("Source and Library can't be the same folder or inside one another".into());
        }
        let stage = staging.clone().unwrap_or_else(default_staging_dir);
        for (label, dir) in [("Source", &source), ("Library", &library)] {
            if let Some(d) = dir
                && overlap(&stage, d)
            {
                return bad(format!(
                    "the staging folder can't be inside {label}, or contain it"
                ));
            }
        }
        if self.keep_originals && library.is_none() {
            return bad("Keep originals needs a Library folder for the processed copies".into());
        }
        Ok(Settings {
            source_dir: source,
            library_dir: library,
            keep_originals: self.keep_originals,
            staging_dir: staging,
        })
    }
}

pub fn load(conn: &Connection) -> Result<Settings, Error> {
    let get = |key: &str| -> Result<Option<String>, Error> {
        Ok(conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    };
    Ok(Settings {
        source_dir: get("source_dir")?.map(PathBuf::from),
        library_dir: get("library_dir")?.map(PathBuf::from),
        keep_originals: get("keep_originals")?.as_deref() == Some("1"),
        staging_dir: get("staging_dir")?.map(PathBuf::from),
    })
}

/// Validate and store. Returns what was stored (absolute paths).
pub fn save(conn: &mut Connection, s: &Settings) -> Result<Settings, Error> {
    let s = s.validated()?;
    let tx = conn.transaction()?;
    {
        let mut put = tx.prepare(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )?;
        let mut del = tx.prepare("DELETE FROM settings WHERE key = ?1")?;
        let path = |p: &Option<PathBuf>| p.as_ref().map(|p| p.to_string_lossy().into_owned());
        for (key, value) in [
            ("source_dir", path(&s.source_dir)),
            ("library_dir", path(&s.library_dir)),
            ("staging_dir", path(&s.staging_dir)),
            (
                "keep_originals",
                Some(if s.keep_originals { "1" } else { "0" }.to_owned()),
            ),
        ] {
            match value {
                Some(v) => put.execute(params![key, v])?,
                None => del.execute([key])?,
            };
        }
    }
    tx.commit()?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let (s, l) = (t.path().join("source"), t.path().join("library"));
        std::fs::create_dir_all(&s).unwrap();
        std::fs::create_dir_all(&l).unwrap();
        (t, s, l)
    }

    #[test]
    fn save_and_load_round_trip() {
        let (t, s, l) = dirs();
        let mut conn = crate::db::open_in_memory().unwrap();
        assert_eq!(load(&conn).unwrap(), Settings::default());
        let stored = save(
            &mut conn,
            &Settings {
                source_dir: Some(s.clone()),
                library_dir: Some(l.clone()),
                keep_originals: true,
                staging_dir: Some(t.path().join("stage")),
            },
        )
        .unwrap();
        assert_eq!(load(&conn).unwrap(), stored);
        assert_eq!(stored.source_dir, Some(s.canonicalize().unwrap()));
        // Clearing the library (and so keep originals) removes them.
        let cleared = save(
            &mut conn,
            &Settings {
                source_dir: Some(s),
                ..Settings::default()
            },
        )
        .unwrap();
        assert_eq!(load(&conn).unwrap(), cleared);
        assert_eq!(cleared.library_dir, None);
        assert!(!cleared.keep_originals);
    }

    #[test]
    fn folders_must_make_sense_together() {
        let (t, s, l) = dirs();
        let err = |s: Settings| s.validated().unwrap_err().to_string();
        assert!(
            err(Settings {
                source_dir: Some(t.path().join("nope")),
                ..Default::default()
            })
            .contains("Source folder")
        );
        assert!(
            err(Settings {
                source_dir: Some(s.clone()),
                library_dir: Some(s.join(".")),
                ..Default::default()
            })
            .contains("inside one another")
        );
        let inner = s.join("sorted");
        std::fs::create_dir(&inner).unwrap();
        assert!(
            err(Settings {
                source_dir: Some(s.clone()),
                library_dir: Some(inner),
                ..Default::default()
            })
            .contains("inside")
        );
        assert!(
            err(Settings {
                source_dir: Some(s.clone()),
                staging_dir: Some(s.join("stage")),
                ..Default::default()
            })
            .contains("staging")
        );
        assert!(
            err(Settings {
                source_dir: Some(s.clone()),
                keep_originals: true,
                ..Default::default()
            })
            .contains("Library")
        );
        assert!(
            Settings {
                source_dir: Some(s),
                library_dir: Some(l),
                keep_originals: true,
                staging_dir: None
            }
            .validated()
            .is_ok()
        );
    }
}
