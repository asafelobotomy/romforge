//! Milestone 11: third-party game databases the user downloads from inside
//! romforge. Free and without an account, but published without a licence,
//! so romforge never ships their data: each user fetches them from the
//! source onto their own disk, and Enrich uses them to fill only what the
//! open sources (libretro, Wikidata) leave empty (house rule 10).
//!
//! - OpenVGDB (OpenEmu's database): matched by SHA1, so strict (house rule
//!   8). Kept as its own SQLite file, with an index added on the hashes.
//! - LaunchBox Games Database (`Metadata.zip`): no hashes, so matched by
//!   exact title (accents and punctuation aside) on the platforms named in
//!   `systems.rs`, and only when exactly one game fits. Read as a stream
//!   from the zip into a small SQLite index of the platforms romforge knows;
//!   its images are fetched one by one when Enrich needs them.

use std::collections::BTreeSet;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use quick_xml::XmlVersion;
use quick_xml::events::Event;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use crate::enrich::ArtKind;
use crate::{Error, fetch, systems};

mod import;
mod launchbox;
mod openvgdb;
#[cfg(test)]
mod tests;

pub use import::*;
pub use launchbox::*;
pub use openvgdb::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtDb {
    OpenVgdb,
    LaunchBox,
}

impl ExtDb {
    pub const ALL: [ExtDb; 2] = [ExtDb::OpenVgdb, ExtDb::LaunchBox];

    pub fn key(self) -> &'static str {
        match self {
            ExtDb::OpenVgdb => "openvgdb",
            ExtDb::LaunchBox => "launchbox",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|d| d.key().eq_ignore_ascii_case(s.trim()))
    }

    pub fn name(self) -> &'static str {
        match self {
            ExtDb::OpenVgdb => "OpenVGDB",
            ExtDb::LaunchBox => "LaunchBox Games Database",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            ExtDb::OpenVgdb => {
                "OpenEmu's database of 52,000 ROMs by hash: descriptions, developer, publisher, genre, release date. \
                 Last updated 2021. About 9 MB."
            }
            ExtDb::LaunchBox => {
                "LaunchBox's community database of 190,000 games, updated daily: overviews, studios, genres, \
                 players, ratings, and box, 3D box, logo, fanart and screenshot images. About 110 MB to download."
            }
        }
    }

    pub fn homepage(self) -> &'static str {
        match self {
            ExtDb::OpenVgdb => "https://github.com/OpenVGDB/OpenVGDB",
            ExtDb::LaunchBox => "https://gamesdb.launchbox-app.com",
        }
    }

    fn file(self) -> &'static str {
        match self {
            ExtDb::OpenVgdb => "openvgdb.sqlite",
            ExtDb::LaunchBox => "launchbox.sqlite",
        }
    }
}

/// Where the databases are fetched from (tests point these elsewhere).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bases {
    pub openvgdb: String,
    pub launchbox: String,
    pub launchbox_images: String,
}

impl Default for Bases {
    fn default() -> Self {
        Bases {
            openvgdb: "https://github.com/OpenVGDB/OpenVGDB/releases/latest/download/openvgdb.zip".into(),
            launchbox: "https://gamesdb.launchbox-app.com/Metadata.zip".into(),
            launchbox_images: "https://images.launchbox-app.com".into(),
        }
    }
}

/// The folder for the downloaded databases: `databases/` next to the
/// library database.
pub fn dir_for(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(crate::layout::LOOSE_DATABASES_DIR)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ExtDbInfo {
    pub db: ExtDb,
    pub name: &'static str,
    pub about: &'static str,
    pub homepage: &'static str,
    /// When it was downloaded, if it was.
    pub downloaded_at: Option<String>,
    pub bytes: Option<u64>,
    /// Games (LaunchBox) or ROMs (OpenVGDB) in the local copy.
    pub entries: Option<i64>,
}

pub fn fold_accents(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let r = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => "a",
            'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' => "A",
            'è' | 'é' | 'ê' | 'ë' | 'ē' => "e",
            'È' | 'É' | 'Ê' | 'Ë' | 'Ē' => "E",
            'ì' | 'í' | 'î' | 'ï' | 'ī' => "i",
            'Ì' | 'Í' | 'Î' | 'Ï' | 'Ī' => "I",
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => "o",
            'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' => "O",
            'ù' | 'ú' | 'û' | 'ü' | 'ū' => "u",
            'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ū' => "U",
            'ñ' => "n",
            'Ñ' => "N",
            'ç' => "c",
            'Ç' => "C",
            'ý' | 'ÿ' => "y",
            'Ý' => "Y",
            'ß' => "ss",
            'æ' => "ae",
            'Æ' => "AE",
            'œ' => "oe",
            'Œ' => "OE",
            _ => {
                out.push(c);
                continue;
            }
        };
        out.push_str(r);
    }
    out
}

/// What a database knows about one game, as `game_meta` fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    pub fields: Vec<(&'static str, String)>,
    /// LaunchBox only: every image, best region first.
    pub images: Vec<crate::enrich::ArtFound>,
}
