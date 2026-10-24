//! The one table of game ID readers: what a cartridge's internal header or
//! a disc's boot files say the game is (milestone 12.3).
//!
//! Each reader checks a structure that can't occur by chance (a checksum,
//! a magic, a boot file) before it answers, so no system is guessed from an
//! extension. An ID is evidence only: never a match on its own (house rule
//! 8), since revisions, regions and discs share codes, and a hack keeps its
//! base game's header.

use crate::disc::SYNC;

mod carts;
use carts::*;
mod discs;
use discs::*;
mod containers;
mod ctr;
pub mod wup;
pub use containers::*;
pub use ctr::{Z3dsForm, z3ds_form};

/// Bytes of the data (in the DAT's form: header stripped, byte order fixed)
/// the readers get. A PS1 disc's `SYSTEM.CNF` sits within the first few
/// hundred sectors.
pub const PREFIX_LEN: usize = 1 << 20;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct GameId {
    /// Which reader answered: `snes`, `md`, `gba`, `gb`, `n64`, `nds`,
    /// `ps1`, `ps2`, `psp`, `saturn`, `segacd`, `dc`, `gc`, `wii`, `3ds`,
    /// `wiiu`.
    pub kind: &'static str,
    /// The product code as the game stores it: a 4-letter game code
    /// (`BPEE`, `NSME`), a Mega Drive serial (`T-95076-00`), a disc's boot
    /// file (`SLUS_007.07`), a GameCube/Wii ID (`GALE01`).
    pub serial: Option<String>,
    /// The internal title, as stored (often upper case, cut short).
    pub title: Option<String>,
    /// Region as the header says it, e.g. `Japan`, `USA`, or a raw code.
    pub region: Option<String>,
    /// For a compressed disc image (RVZ, WIA, GCZ, CSO, ZSO): the size of
    /// the plain image it holds, to compare with the DAT's.
    #[serde(default)]
    pub image_size: Option<u64>,
    /// For an NKit image: the CRC32 its header gives for the original
    /// image (the 4 bytes as stored; their byte order isn't documented).
    #[serde(default)]
    pub source_crc: Option<u32>,
}

impl GameId {
    /// Back from the `files` columns (`id_kind`, ...); `None` without a kind.
    pub fn from_columns(
        kind: Option<String>,
        serial: Option<String>,
        title: Option<String>,
        region: Option<String>,
        image_size: Option<u64>,
        source_crc: Option<u32>,
    ) -> Option<GameId> {
        let kind = KINDS.iter().find(|k| Some(**k) == kind.as_deref())?;
        Some(GameId {
            kind,
            serial,
            title,
            region,
            image_size,
            source_crc,
        })
    }
}

/// Every `kind` a reader gives.
const KINDS: &[&str] = &[
    "snes", "md", "gba", "gb", "n64", "nds", "ps1", "ps2", "psp", "saturn", "segacd", "dc", "gc", "wii",
    "3ds", "wiiu",
];

/// Cartridges before discs; the first reader that answers wins.
type Reader = fn(&[u8]) -> Option<GameId>;

/// The 3DS's and Wii U's first: their headers are told by magic numbers,
/// not guesses.
const READERS: &[Reader] = &[
    ctr::ctr,
    wup::wud,
    gba,
    nds,
    gb,
    n64,
    md,
    snes,
    disc,
    gamecube_wii,
];

/// What the first bytes of a file say it is, if any reader recognises them.
pub fn read(prefix: &[u8]) -> Option<GameId> {
    let id = READERS.iter().find_map(|r| r(prefix))?;
    debug_assert!(KINDS.contains(&id.kind), "{} not in KINDS", id.kind);
    Some(id)
}

/// Printable text from a fixed-size field: stops at NUL, trims spaces.
fn text(bytes: &[u8]) -> Option<String> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let s: String = bytes[..end]
        .iter()
        .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { ' ' })
        .collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    (!s.is_empty()).then_some(s)
}

/// A game code: letters and digits only, exactly as long as the field.
fn code(bytes: &[u8]) -> Option<String> {
    bytes
        .iter()
        .all(u8::is_ascii_alphanumeric)
        .then(|| String::from_utf8_lossy(bytes).into_owned())
}

fn id(kind: &'static str, serial: Option<String>, title: Option<String>, region: Option<&str>) -> GameId {
    GameId {
        kind,
        serial,
        title,
        region: region.map(str::to_owned),
        image_size: None,
        source_crc: None,
    }
}

/// The region a Nintendo game code's last letter stands for.
fn code_region(c: u8) -> Option<&'static str> {
    Some(match c {
        b'J' => "Japan",
        b'E' => "USA",
        b'P' => "Europe",
        b'D' => "Germany",
        b'F' => "France",
        b'I' => "Italy",
        b'S' => "Spain",
        b'U' => "Australia",
        b'K' => "Korea",
        b'C' => "China",
        b'A' => "World",
        _ => return None,
    })
}

#[cfg(test)]
mod tests;
