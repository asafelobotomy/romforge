//! Milestone 9, second stage: art from sources that need the owner's keys
//! (house rule 10: ScreenScraper is tier 2, SteamGridDB and IGDB tier 3).
//! They only fill art libretro and the downloaded LaunchBox database don't
//! have (they are asked after both).
//!
//! - [`screenscraper`] by the ROM's hashes: pictures and text.
//! - [`steamgriddb`] by title: hero, logo, icon.
//! - [`igdb`] by title and platform: cover, artwork, screenshot, summary.

use rusqlite::{Connection, OptionalExtension, params};

use crate::Error;
use crate::enrich::ArtFound;
use crate::fetch;
use crate::settings::Keys;
use crate::systems::System;

mod credentials;
mod igdb;
mod screenscraper;
mod steamgriddb;
pub use credentials::{download_url, without_credentials};

/// Where each source lives (changed only in tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bases {
    pub screenscraper: String,
    pub steamgriddb: String,
    pub igdb_token: String,
    pub igdb: String,
    pub igdb_images: String,
}

impl Default for Bases {
    fn default() -> Self {
        Bases {
            screenscraper: "https://api.screenscraper.fr/api2".into(),
            steamgriddb: "https://www.steamgriddb.com/api/v2".into(),
            igdb_token: "https://id.twitch.tv/oauth2/token".into(),
            igdb: "https://api.igdb.com/v4".into(),
            igdb_images: "https://images.igdb.com/igdb/image/upload".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    ScreenScraper,
    SteamGridDb,
    Igdb,
}

impl Provider {
    /// It has text for a game too (a synopsis, genres…), not only pictures.
    pub fn has_text(self) -> bool {
        matches!(self, Provider::ScreenScraper | Provider::Igdb)
    }

    pub fn name(self) -> &'static str {
        match self {
            Provider::ScreenScraper => "screenscraper",
            Provider::SteamGridDb => "steamgriddb",
            Provider::Igdb => "igdb",
        }
    }

    /// The sources usable for `system` with `keys`, in house rule 10 order.
    pub fn available(keys: &Keys, system: &System) -> Vec<Provider> {
        let mut out = Vec::new();
        if keys.screenscraper() && system.screenscraper.is_some() {
            out.push(Provider::ScreenScraper);
        }
        if keys.steamgriddb() {
            out.push(Provider::SteamGridDb);
        }
        if keys.igdb() && !system.igdb.is_empty() {
            out.push(Provider::Igdb);
        }
        out
    }
}

/// One ROM of a game, for hash lookups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomRef {
    pub name: String,
    pub size: u64,
    pub crc32: String,
    pub md5: String,
    pub sha1: String,
}

/// The first matched (or hand-named) file of a game.
pub fn rom_for(conn: &Connection, system_id: i64, title: &str) -> Result<Option<RomRef>, Error> {
    let row = |r: &rusqlite::Row| -> rusqlite::Result<RomRef> {
        let path: String = r.get(0)?;
        let member: String = r.get(1)?;
        let shown = if member.is_empty() { path } else { member };
        Ok(RomRef {
            name: shown.rsplit('/').next().unwrap_or(&shown).to_owned(),
            size: r.get(2)?,
            crc32: r.get(3)?,
            md5: r.get(4)?,
            sha1: r.get(5)?,
        })
    };
    let hashed = conn
        .query_row(
            concat!(
                "SELECT f.path, f.member, f.size, f.crc32, f.md5, f.sha1
             FROM files f JOIN matches m ON m.file_id = f.file_id AND m.via IN ",
                crate::report::strong!(),
                "
             JOIN roms r ON r.rom_id = m.rom_id JOIN games g ON g.game_id = r.game_id
             JOIN dats d ON d.dat_id = g.dat_id
             WHERE d.system_id = ?1 AND g.name = ?2 AND f.status IN ",
                crate::report::holding!(),
                "
             ORDER BY f.size DESC LIMIT 1"
            ),
            params![system_id, title],
            row,
        )
        .optional()?;
    if hashed.is_some() {
        return Ok(hashed);
    }
    Ok(conn
        .query_row(
            "SELECT f.path, f.member, f.size, f.crc32, f.md5, f.sha1
             FROM files f JOIN manual_ids mi ON mi.sha1 = f.sha1 AND mi.size = f.size
             WHERE mi.system_id = ?1 AND mi.title = ?2 ORDER BY f.size DESC LIMIT 1",
            params![system_id, title],
            row,
        )
        .optional()?)
}

/// What one game needs looked up.
pub struct Job<'a> {
    pub system: &'a System,
    pub title: &'a str,
    pub rom: Option<RomRef>,
    /// Regions, best first (Settings): whose pictures come first.
    pub prefs: &'a [String],
    /// The language for text (Settings).
    pub language: &'a str,
    /// The game's release year, if known: tells same-named games apart.
    pub year: Option<i32>,
}

/// What a source has for a game: its pictures, the best of each kind
/// first, and fields of text (`description`, `genre`…).
#[derive(Debug, Clone, Default)]
pub struct Found {
    pub art: Vec<ArtFound>,
    pub fields: Vec<(&'static str, String)>,
}

/// Remembers an IGDB token for the rest of a run.
#[derive(Default)]
pub struct Session {
    igdb_token: Option<String>,
}

/// What `provider` has for the job.
pub fn lookup(
    provider: Provider,
    keys: &Keys,
    bases: &Bases,
    session: &mut Session,
    job: &Job,
) -> Result<Found, Error> {
    match provider {
        Provider::ScreenScraper => screenscraper::lookup(keys, bases, job),
        Provider::SteamGridDb => steamgriddb::lookup(keys, bases, job),
        Provider::Igdb => igdb::lookup(keys, bases, session, job),
    }
}

fn json(body: &[u8]) -> Result<serde_json::Value, Error> {
    serde_json::from_slice(body).map_err(|e| Error::Http(format!("unexpected answer: {e}")))
}
