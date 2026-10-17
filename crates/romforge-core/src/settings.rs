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
//! - **Keys**: optional credentials for ScreenScraper, SteamGridDB and IGDB
//!   (house rule 10). Kept in the local database, shown masked.
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
    /// Optional credentials for online sources (house rule 10, tiers 2 and
    /// 3). Stored in the local database; never shown in full.
    #[serde(default)]
    pub keys: Keys,
    /// Details shows a box for every kind of art, even ones with nothing
    /// in them (off: only the art there is).
    #[serde(default)]
    pub show_empty_media: bool,
}

/// Credentials for the online sources that need them. All optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Keys {
    /// ScreenScraper developer credentials (issued to an app).
    pub screenscraper_dev_id: Option<String>,
    pub screenscraper_dev_password: Option<String>,
    /// ScreenScraper user account (optional; raises the request limits).
    pub screenscraper_user: Option<String>,
    pub screenscraper_password: Option<String>,
    pub steamgriddb_key: Option<String>,
    /// IGDB goes through a Twitch application.
    pub igdb_client_id: Option<String>,
    pub igdb_client_secret: Option<String>,
}

impl Keys {
    /// (setting key, value) pairs, in a fixed order.
    fn fields(&self) -> [(&'static str, Option<String>); 7] {
        [
            ("screenscraper_dev_id", self.screenscraper_dev_id.clone()),
            (
                "screenscraper_dev_password",
                self.screenscraper_dev_password.clone(),
            ),
            ("screenscraper_user", self.screenscraper_user.clone()),
            ("screenscraper_password", self.screenscraper_password.clone()),
            ("steamgriddb_key", self.steamgriddb_key.clone()),
            ("igdb_client_id", self.igdb_client_id.clone()),
            ("igdb_client_secret", self.igdb_client_secret.clone()),
        ]
    }

    fn trimmed(&self) -> Keys {
        let t = |v: &Option<String>| v.as_ref().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
        Keys {
            screenscraper_dev_id: t(&self.screenscraper_dev_id),
            screenscraper_dev_password: t(&self.screenscraper_dev_password),
            screenscraper_user: t(&self.screenscraper_user),
            screenscraper_password: t(&self.screenscraper_password),
            steamgriddb_key: t(&self.steamgriddb_key),
            igdb_client_id: t(&self.igdb_client_id),
            igdb_client_secret: t(&self.igdb_client_secret),
        }
    }

    pub fn screenscraper(&self) -> bool {
        self.screenscraper_dev_id.is_some() && self.screenscraper_dev_password.is_some()
    }

    pub fn steamgriddb(&self) -> bool {
        self.steamgriddb_key.is_some()
    }

    pub fn igdb(&self) -> bool {
        self.igdb_client_id.is_some() && self.igdb_client_secret.is_some()
    }
}

/// `abc…xyz` for display: enough to recognize a key, not to use it.
pub fn mask(secret: &str) -> String {
    let n = secret.chars().count();
    if n <= 8 {
        "•".repeat(n.max(4))
    } else {
        let head: String = secret.chars().take(3).collect();
        let tail: String = secret.chars().skip(n - 3).collect();
        format!("{head}…{tail}")
    }
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
        let keys = self.keys.trimmed();
        let pair = |a: &Option<String>, b: &Option<String>, what: &str| {
            if a.is_some() != b.is_some() {
                Err(Error::Settings(format!("{what} needs both fields")))
            } else {
                Ok(())
            }
        };
        pair(
            &keys.screenscraper_dev_id,
            &keys.screenscraper_dev_password,
            "ScreenScraper developer login",
        )?;
        pair(
            &keys.screenscraper_user,
            &keys.screenscraper_password,
            "ScreenScraper user login",
        )?;
        pair(
            &keys.igdb_client_id,
            &keys.igdb_client_secret,
            "IGDB (Twitch client ID and secret)",
        )?;
        Ok(Settings {
            source_dir: source,
            library_dir: library,
            keep_originals: self.keep_originals,
            staging_dir: staging,
            keys,
            show_empty_media: self.show_empty_media,
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
        keys: Keys {
            screenscraper_dev_id: get("screenscraper_dev_id")?,
            screenscraper_dev_password: get("screenscraper_dev_password")?,
            screenscraper_user: get("screenscraper_user")?,
            screenscraper_password: get("screenscraper_password")?,
            steamgriddb_key: get("steamgriddb_key")?,
            igdb_client_id: get("igdb_client_id")?,
            igdb_client_secret: get("igdb_client_secret")?,
        },
        show_empty_media: get("show_empty_media")?.as_deref() == Some("1"),
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
            (
                "show_empty_media",
                Some(if s.show_empty_media { "1" } else { "0" }.to_owned()),
            ),
        ]
        .into_iter()
        .chain(s.keys.fields())
        {
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
                ..Default::default()
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
    fn keys_are_trimmed_paired_and_masked() {
        let mut conn = crate::db::open_in_memory().unwrap();
        let mut s = Settings::default();
        s.keys.steamgriddb_key = Some("  abcdef0123456789  ".into());
        s.keys.igdb_client_id = Some("id".into());
        assert!(save(&mut conn, &s).unwrap_err().to_string().contains("IGDB"));
        s.keys.igdb_client_secret = Some("secret".into());
        s.keys.screenscraper_user = Some("".into());
        let stored = save(&mut conn, &s).unwrap();
        assert_eq!(stored.keys.steamgriddb_key.as_deref(), Some("abcdef0123456789"));
        assert_eq!(stored.keys.screenscraper_user, None, "empty means unset");
        assert!(stored.keys.igdb() && stored.keys.steamgriddb() && !stored.keys.screenscraper());
        assert_eq!(load(&conn).unwrap(), stored);
        assert_eq!(mask("abcdef0123456789"), "abc…789");
        assert_eq!(mask("short"), "•••••");
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
                ..Default::default()
            }
            .validated()
            .is_ok()
        );
    }
}
