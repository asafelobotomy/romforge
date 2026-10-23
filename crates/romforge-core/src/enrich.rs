//! Milestone 3: metadata and art for the games you have, cached locally.
//!
//! In house rule 10's order, open sources first:
//! - metadata from libretro-database's `metadat/<field>/<system>.dat`
//!   (genre, developer, publisher, franchise, release year and month, max
//!   players; some entries also carry a description or serial), then
//!   Wikidata ([`crate::wikidata`]), then what both left empty from the
//!   databases the user downloaded, OpenVGDB and LaunchBox
//!   ([`crate::extdb`]);
//! - art from `thumbnails.libretro.com/<system>/Named_<kind>s/<game>.png`
//!   (box art, title screens, in-game snaps), saved under the cache folder;
//!   what libretro lacks from LaunchBox's pictures, then from the keyed
//!   sources in `artsources.rs` (also hero, logo, icon, 3D box), when the
//!   owner entered keys.
//!
//! Records are matched to a game by its exact DAT name, else by the CRC32
//! of one of its ROMs within that system's file. Only games with a matched
//! file are looked up. Nothing is written next to the ROMs.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::{Error, dat, fetch, systems};

mod art;
mod choices;
mod chosen;
mod fields;
mod icons;
mod info;
mod meta;
pub mod regions;
mod run;
#[cfg(test)]
mod tests;
mod types;

pub use art::*;
pub use choices::*;
pub use fields::*;
pub use icons::*;
pub use info::*;
pub(crate) use meta::libretro_game;
use meta::*;
pub use run::*;
pub use types::*;

pub const THUMBNAILS_BASE: &str = "https://thumbnails.libretro.com";

/// The game's name in libretro's database, kept when it differs from the
/// DAT's (matched by CRC): libretro's thumbnails are named by it.
pub const LIBRETRO_NAME: &str = "libretro_name";

/// The metadata files fetched per system (`metadat/<file>/<system>.dat`;
/// a system without one has none of it).
pub const FIELDS: &[&str] = &[
    "genre",
    "developer",
    "publisher",
    "franchise",
    "releaseyear",
    "releasemonth",
    "maxusers",
    "esrb",
    "serial",
    "origin",
    "rumble",
    "analog",
    "magazine/edge",
    "magazine/famitsu",
];

/// When romforge last learnt to ask its sources for more (fields, art
/// kinds): a game's metadata checked, a libretro picture not found, or a
/// keyed source asked before then is looked up again, once (the stamps
/// are `db::now!()`'s format).
pub const ASK_AGAIN_SINCE: &str = "2026-10-23T12:00:00Z";

/// The field a libretro record's key is kept as: the names romforge uses
/// for it elsewhere (LaunchBox's `esrb`).
pub fn libretro_field(key: &str) -> &str {
    match key {
        "esrb_rating" => "esrb",
        "edge_rating" => "edge",
        "famitsu_rating" => "famitsu",
        k => k,
    }
}
