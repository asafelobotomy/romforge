//! Milestone 3: metadata and art for the games you have, cached locally.
//!
//! Both come from libretro, needing no account:
//! - metadata from libretro-database's `metadat/<field>/<system>.dat`
//!   (genre, developer, publisher, franchise, release year and month, max
//!   players; some entries also carry a description or serial);
//! - art from `thumbnails.libretro.com/<system>/Named_<kind>s/<game>.png`
//!   (box art, title screens, in-game snaps), saved under the cache folder;
//!   what libretro lacks (and hero, logo, icon, 3D box) from the keyed
//!   sources in `artsources.rs`, when the owner entered keys.
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
mod icons;
mod info;
mod meta;
mod run;
#[cfg(test)]
mod tests;
mod types;

pub use art::*;
pub use choices::*;
pub use icons::*;
pub use info::*;
use meta::*;
pub use run::*;
pub use types::*;

pub const THUMBNAILS_BASE: &str = "https://thumbnails.libretro.com";

/// The game's name in libretro's database, kept when it differs from the
/// DAT's (matched by CRC): libretro's thumbnails are named by it.
pub const LIBRETRO_NAME: &str = "libretro_name";

/// The metadata files fetched per system.
pub const FIELDS: &[&str] = &[
    "genre",
    "developer",
    "publisher",
    "franchise",
    "releaseyear",
    "releasemonth",
    "maxusers",
];
