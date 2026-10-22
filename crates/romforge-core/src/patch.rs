//! ROM patches: IPS, BPS and UPS (milestone 12.7), PPF and xdelta
//! (VCDIFF, milestone 19).
//!
//! BPS and UPS name what they were made from (source size and CRC32) and
//! what they make; IPS, PPF and xdelta say nothing that checks the source
//! (a PPF may carry a block of the image to compare). Applying a patch to a
//! hash-verified copy of its base and comparing the result's SHA1 with a
//! file proves the file is that patch of that game: still never "verified"
//! (house rule 8), but no longer a guess. Patches are never written; the
//! patcher (milestone 19) applies one into a new file, never over the
//! original: in memory for a cartridge, [`apply_file`] streamed for a disc.

mod file;
mod finder;
mod formats;
#[cfg(test)]
pub(crate) mod tests;

pub use file::apply_file;
pub use finder::is_patch_name;
pub(crate) use finder::{PatchFinder, read_entry};
pub(crate) use formats::ppf_records;
use formats::{apply_bps, apply_ips, apply_ups};

/// The patch formats romforge reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ips,
    Bps,
    Ups,
    /// PlayStation Patch Format, versions 1 to 3.
    Ppf,
    /// VCDIFF (RFC 3284) as xdelta3 writes it.
    Xdelta,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Ips => "IPS",
            Kind::Bps => "BPS",
            Kind::Ups => "UPS",
            Kind::Ppf => "PPF",
            Kind::Xdelta => "xdelta",
        }
    }
}

/// File extensions of patches, lower case.
pub const EXTENSIONS: &[&str] = &["bps", "ups", "ips", "ppf", "xdelta", "vcdiff"];

/// What a patch's own header and footer say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Info {
    pub kind: Kind,
    /// BPS / UPS: the source's and target's size and CRC32.
    pub source: Option<(u64, u32)>,
    pub target: Option<(u64, u32)>,
}

/// Biggest source or target romforge applies a patch to (cartridges).
pub(crate) const MAX_SIZE: u64 = 64 << 20;

/// Read a patch's header (and for BPS / UPS its footer, whose own CRC32
/// must check out).
pub fn info(data: &[u8]) -> Option<Info> {
    if ppf_records(data).is_some() {
        return Some(Info {
            kind: Kind::Ppf,
            source: None,
            target: None,
        });
    }
    if data.starts_with(&oxidelta::vcdiff::VCDIFF_MAGIC) {
        return Some(Info {
            kind: Kind::Xdelta,
            source: None,
            target: None,
        });
    }
    if data.starts_with(b"PATCH") && data.ends_with(b"EOF") || ips_end(data).is_some() {
        return Some(Info {
            kind: Kind::Ips,
            source: None,
            target: None,
        });
    }
    let kind = if data.starts_with(b"BPS1") {
        Kind::Bps
    } else if data.starts_with(b"UPS1") {
        Kind::Ups
    } else {
        return None;
    };
    if data.len() < 4 + 12 {
        return None;
    }
    let footer = &data[data.len() - 12..];
    if crc32fast::hash(&data[..data.len() - 4]) != le32(&footer[8..]) {
        return None;
    }
    let mut at = 4;
    let source_size = number(data, &mut at)?;
    let target_size = number(data, &mut at)?;
    Some(Info {
        kind,
        source: Some((source_size, le32(&footer[..4]))),
        target: Some((target_size, le32(&footer[4..8]))),
    })
}

/// Where an IPS file's records end ("EOF", maybe followed by a 3-byte
/// truncation size).
fn ips_end(data: &[u8]) -> Option<usize> {
    if !data.starts_with(b"PATCH") {
        return None;
    }
    let mut at = 5;
    loop {
        let head = data.get(at..at + 3)?;
        if head == b"EOF" {
            return Some(at);
        }
        let size = u16::from_be_bytes([*data.get(at + 3)?, *data.get(at + 4)?]) as usize;
        at += 5;
        at += if size == 0 { 3 } else { size };
    }
}

fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// BPS / UPS variable-length number: 7 bits a byte, the high bit ends it.
fn number(data: &[u8], at: &mut usize) -> Option<u64> {
    let (mut value, mut shift) = (0u64, 1u64);
    loop {
        let b = *data.get(*at)?;
        *at += 1;
        value = value.checked_add(((b & 0x7f) as u64).checked_mul(shift)?)?;
        if b & 0x80 != 0 {
            return Some(value);
        }
        // `checked_shl` only checks the shift amount, not lost bits.
        shift = shift.checked_mul(128)?;
        value = value.checked_add(shift)?;
    }
}

/// Apply a patch to `source` in memory. `None` when it doesn't fit (a
/// BPS / UPS made from something else, a damaged patch, too big).
pub fn apply(patch: &[u8], source: &[u8]) -> Option<Vec<u8>> {
    let i = info(patch)?;
    if source.len() as u64 > MAX_SIZE {
        return None;
    }
    if let Some((size, crc)) = i.source
        && (size != source.len() as u64 || crc != crc32fast::hash(source))
    {
        return None;
    }
    let out = match i.kind {
        Kind::Ips => apply_ips(patch, source)?,
        Kind::Bps => apply_bps(patch, source)?,
        Kind::Ups => apply_ups(patch, source)?,
        Kind::Ppf => {
            let ppf = ppf_records(patch)?;
            if !ppf.fits(source) {
                return None;
            }
            let mut out = source.to_vec();
            for (at, data) in &ppf.records {
                let end = (*at as usize).checked_add(data.len())?;
                if end > MAX_SIZE as usize {
                    return None;
                }
                if end > out.len() {
                    out.resize(end, 0);
                }
                out[*at as usize..end].copy_from_slice(data);
            }
            out
        }
        Kind::Xdelta => oxidelta::vcdiff::decode_memory(&crate::vcdiff::plain(patch).ok()?, source).ok()?,
    };
    if let Some((size, crc)) = i.target
        && (size != out.len() as u64 || crc != crc32fast::hash(&out))
    {
        return None;
    }
    Some(out)
}
