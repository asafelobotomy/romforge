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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::Error;

mod keys;
pub use keys::*;
mod store;
pub use store::*;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub source_dir: Option<PathBuf>,
    pub library_dir: Option<PathBuf>,
    pub keep_originals: bool,
    /// Rename & Cleanup's "Replace originals": each game is renamed and
    /// repacked in its own folder (not moved into RomForge or the Library),
    /// its original to the trash. Never with `keep_originals`.
    #[serde(default)]
    pub in_place: bool,
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
    /// Which region's version is a game's main row in the library, best
    /// first ("USA", "Europe", "Japan"...). Empty: [`crate::organize::DEFAULT_REGIONS`].
    #[serde(default)]
    pub regions: Vec<String>,
    /// The sidebar's platform icons ([`crate::enrich::PLATFORM_ICON_THEMES`]:
    /// controllers, consoles or logos), or "none" for plain badges. Empty:
    /// "flatui", the controllers.
    #[serde(default)]
    pub platform_icons: String,
    /// Which romforge logo the app shows ([`LOGOS`]). Empty: "synthwave".
    #[serde(default)]
    pub logo: String,
    /// The program to start for an emulator (by its name in
    /// [`crate::launch::known`]), found before any other copy.
    #[serde(default)]
    pub emulator_paths: BTreeMap<String, PathBuf>,
}

/// The app's logos (its artwork in `app/src/assets/logos/`).
pub const LOGOS: &[&str] = &["synthwave", "pastel", "gummy", "space", "sprinkles"];

impl Settings {
    /// The icon theme to use, or `None` for none.
    pub fn icon_theme(&self) -> Option<&str> {
        match self.platform_icons.as_str() {
            "" => Some("flatui"),
            "none" => None,
            t => Some(t),
        }
    }
}

/// `<cache>/staging`, the cleanup scratch folder unless one is set.
pub fn default_staging_dir() -> PathBuf {
    crate::enrich::default_cache_dir().join("staging")
}

impl Settings {
    /// [`Settings::regions`], or the default order when none is set.
    pub fn region_order(&self) -> Vec<String> {
        if self.regions.is_empty() {
            crate::organize::DEFAULT_REGIONS
                .iter()
                .map(|r| (*r).to_owned())
                .collect()
        } else {
            self.regions.clone()
        }
    }

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
        if self.in_place && self.keep_originals {
            return bad("Replace originals puts the originals in the trash; it can't keep them".into());
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
            in_place: self.in_place,
            staging_dir: staging,
            keys,
            show_empty_media: self.show_empty_media,
            logo: {
                let l = self.logo.trim();
                if !l.is_empty() && !LOGOS.contains(&l) {
                    return bad(format!("no logo {l:?}"));
                }
                l.to_owned()
            },
            platform_icons: {
                let t = self.platform_icons.trim();
                if !t.is_empty() && t != "none" && !crate::enrich::PLATFORM_ICON_THEMES.contains(&t) {
                    return bad(format!("no platform icon theme {t:?}"));
                }
                t.to_owned()
            },
            regions: self
                .regions
                .iter()
                .map(|r| r.trim().to_owned())
                .filter(|r| !r.is_empty())
                .collect(),
            emulator_paths: self.emulator_paths()?,
        })
    }

    /// [`Settings::emulator_paths`] checked: known emulators, existing
    /// programs, absolute paths. An empty path drops the entry.
    fn emulator_paths(&self) -> Result<BTreeMap<String, PathBuf>, Error> {
        let mut out = BTreeMap::new();
        for (name, path) in &self.emulator_paths {
            if path.as_os_str().is_empty() {
                continue;
            }
            let e = crate::launch::known(name.trim())
                .ok_or_else(|| Error::Settings(format!("no emulator named {name:?}")))?;
            let p = std::fs::canonicalize(path)
                .map_err(|err| Error::Settings(format!("{} program {}: {err}", e.name, path.display())))?;
            if !p.is_file() {
                return Err(Error::Settings(format!("{} is not a program", p.display())));
            }
            out.insert(e.name.to_owned(), p);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests;
