//! DAT parsing: Logiqx XML (No-Intro, Redump, TOSEC and MAME-style
//! `<machine>`) and the older clrmamepro text format (`game ( rom ( ... ) )`).

use std::io::BufRead;

use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

use crate::Error;

mod xml;
pub use xml::*;
mod cmp;
pub use cmp::*;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Dat {
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub homepage: Option<String>,
    pub url: Option<String>,
    /// `<clrmamepro header="No-Intro_NES.xml"/>`: the DAT's hashes are of
    /// header-stripped data.
    pub header_hint: Option<String>,
    pub games: Vec<Game>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Game {
    pub name: String,
    pub description: Option<String>,
    /// The parent game's name (`cloneof`), when the DAT groups versions.
    pub cloneof: Option<String>,
    /// e.g. "USA" (libretro's No-Intro mirror; Redump has it in the name).
    pub region: Option<String>,
    /// Redump's category: "Games", "Demos", "Applications", ...
    pub category: Option<String>,
    /// Product codes (Redump with `serial` asked for, some libretro DATs).
    /// Several discs or versions can share one, so a serial only narrows.
    pub serials: Vec<String>,
    pub roms: Vec<Rom>,
}

/// "SLUS-00067, SLUS-00067GHA" -> both.
pub fn split_serials(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A serial as compared: letters and digits only, upper case, so a disc's
/// `SLUS_000.67`, Redump's `SLUS-00067` and `SCPS 45196` meet.
pub fn serial_key(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Rom {
    pub name: String,
    pub size: Option<u64>,
    pub crc32: Option<String>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
    /// e.g. `baddump`, `nodump`, `verified`; `optional` for a ROM a game
    /// runs without (MAME's `optional="yes"`).
    pub status: Option<String>,
    /// A disk (MAME's `<disk>`): a CHD, matched by the SHA1 its header
    /// keeps; `name` ends in `.chd` and `size` is unknown.
    pub disk: bool,
}

/// Parse either format, telling them apart by the first non-blank byte.
pub fn parse_any(bytes: &[u8]) -> Result<Dat, Error> {
    let start = bytes.iter().position(|b| !b.is_ascii_whitespace());
    match start.map(|i| bytes[i]) {
        Some(b'<') => parse(&*escape_bare_ampersands(bytes)),
        // Skip a UTF-8 BOM.
        Some(0xef) if bytes[start.unwrap_or(0)..].starts_with(b"\xef\xbb\xbf<") => {
            parse(&*escape_bare_ampersands(bytes))
        }
        _ => {
            let text = String::from_utf8_lossy(bytes);
            let looks_like_cmp = text
                .split_whitespace()
                .next()
                .is_some_and(|w| matches!(w, "clrmamepro" | "game" | "machine" | "resource"));
            if !looks_like_cmp {
                return Err(Error::Dat(
                    "not a DAT (neither Logiqx XML nor clrmamepro text)".into(),
                ));
            }
            parse_cmp(&text)
        }
    }
}

/// Redump leaves `&` unescaped in serials (`H&D/CDCRYST/E`). A bare `&` is
/// never valid XML, so escaping it changes nothing else.
fn escape_bare_ampersands(bytes: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    let is_ref = |rest: &[u8]| {
        let end = rest.iter().take(12).position(|&b| b == b';');
        end.is_some_and(|n| n > 0 && rest[..n].iter().all(|b| b.is_ascii_alphanumeric() || *b == b'#'))
    };
    let bare = |i: usize| bytes[i] == b'&' && !is_ref(&bytes[i + 1..]);
    if !(0..bytes.len()).any(bare) {
        return std::borrow::Cow::Borrowed(bytes);
    }
    let mut out = Vec::with_capacity(bytes.len() + 64);
    for (i, &b) in bytes.iter().enumerate() {
        out.push(b);
        if bare(i) {
            out.extend_from_slice(b"amp;");
        }
    }
    std::borrow::Cow::Owned(out)
}

#[cfg(test)]
mod tests;
