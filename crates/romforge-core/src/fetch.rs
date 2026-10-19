//! Downloading DATs.
//!
//! Where each system's DATs come from is in [`crate::systems`]:
//! - `nointro`: No-Intro's data may be reused freely, but DAT-o-MATIC hands
//!   DATs out only through its download form, so romforge takes them from
//!   libretro-database's public mirror on GitHub instead. For DATs downloaded
//!   from DAT-o-MATIC by hand, the public `profile.xml` says which are out of
//!   date.
//! - `redump`: Redump serves DATs at `http://redump.org/datfile/<code>/`
//!   (game DATs as zips, BIOS DATs as plain clrmamepro text); with
//!   `serial,version` after it, each game also lists its serials.
//! - `tosec`: TOSEC publishes one zip of all its DATs per release (about
//!   100 MB). The latest release is found on tosecdev.org's download page;
//!   the pack is fetched at most once per batch and not at all when its
//!   DATs for a system are already imported.
//! - `hacks`, `homebrew`: libretro-database's own small DATs, fetched only
//!   when asked for ([`sources`] with `extras`).
//! - serials: libretro-database's `metadat/serial` lists (title, serial,
//!   CRC32) for cartridge systems, beside their No-Intro DATs. Discs get
//!   theirs from Redump.
//! - clone lists: Retool's (BSD-3-Clause, on GitHub), which group titles
//!   that are one game under different names, for 1G1R.
//!
//! Requests to one host are spaced out ([`delay_for`]; DAT-o-MATIC's
//! robots.txt asks for 5 s). Downloads stay in memory and are never written
//! to disk.

use std::collections::HashMap;
use std::io::{BufRead, Cursor, Read};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use quick_xml::XmlVersion;
use quick_xml::events::Event;
use ureq::tls::{RootCerts, TlsConfig};

use crate::Error;
use crate::systems::System;

mod dats;
mod request;
mod sources;
#[cfg(test)]
mod tests;

pub use dats::*;
pub use request::*;
pub use sources::*;

/// Wikimedia's User-Agent policy: a name, a version and a contact. Without
/// the contact, Wikimedia's edge may treat romforge as an anonymous client
/// (in October 2026 it held those to one request a minute).
pub const USER_AGENT: &str = concat!(
    "romforge/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/asafelobotomy/romforge)"
);
pub const REDUMP_BASE: &str = "http://redump.org";
pub const LIBRETRO_BASE: &str = "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat";
pub const LIBRETRO_DAT_BASE: &str = "https://raw.githubusercontent.com/libretro/libretro-database/master/dat";
pub const LIBRETRO_NOINTRO_BASE: &str =
    "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/no-intro";
pub const TOSEC_BASE: &str = "https://www.tosecdev.org";
pub const NOINTRO_PROFILE_URL: &str = "https://datomatic.no-intro.org/profile.xml";
pub const NOINTRO_DOWNLOAD_PAGE: &str = "https://datomatic.no-intro.org/index.php?page=download";
/// No DAT pack is near this; it only stops a runaway response.
const MAX_BODY: u64 = 512 << 20;

/// No DAT unpacks to this; it stops a zip bomb.
const MAX_UNPACKED: u64 = 1 << 30;
