//! Milestone 23: RVZ, Dolphin's compressed GameCube and Wii disc format,
//! from Dolphin's `docs/WiaAndRvz.md` only (Dolphin is GPL; nothing of its
//! code is used). All integers big-endian:
//! - `wia_file_head_t` at 0 (0x48): `RVZ\x01`, version, compatible
//!   version, the disc struct's size and SHA-1, the ISO's size, the file's
//!   size, and this struct's own SHA-1;
//! - `wia_disc_t` at 0x48 (0xDC): disc type (1 GameCube, 2 Wii),
//!   compression (5 zstd), level, chunk size, the disc's first 0x80 bytes,
//!   Wii partitions, and where the raw-data and group tables are (both
//!   compressed);
//! - `wia_raw_data_t`: a disc range and its groups; `rvz_group_t`: where a
//!   chunk's data is (offset / 4), its size (top bit: compressed), and its
//!   size before RVZ unpacking (0: not packed).
//!
//! Packed data is runs of a `u32` size: plain bytes, or (top bit set) a
//! 68-byte seed whose padding fills the run ([`junk`]).
//!
//! romforge writes GameCube discs ([`write`]) and reads GameCube discs back
//! as their ISO ([`read`]); a Wii RVZ keeps its partitions decrypted, and
//! rebuilding its ISO would take the Wii common key, so it isn't read.

pub mod gc;
pub mod junk;
pub mod read;
pub mod write;

pub use read::Rvz;

pub const MAGIC: [u8; 4] = *b"RVZ\x01";
/// Dolphin's RVZ version and the oldest version that reads it.
const VERSION: u32 = 0x0100_0000;
const VERSION_COMPATIBLE: u32 = 0x0003_0000;
const HEAD_LEN: usize = 0x48;
const DISC_LEN: usize = 0xDC;
const PART_LEN: u32 = 0x30;
const RAW_LEN: usize = 24;
const GROUP_LEN: usize = 12;
const ZSTD: u32 = 5;
pub const GAMECUBE: u32 = 1;

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap_or_default())
}

fn be64(b: &[u8], at: usize) -> u64 {
    u64::from_be_bytes(b[at..at + 8].try_into().unwrap_or_default())
}

/// `wia_disc_t`'s fields romforge uses.
#[derive(Debug, Clone)]
struct Disc {
    disc_type: u32,
    compression: u32,
    chunk: u64,
    dhead: [u8; 0x80],
    n_raw: u32,
    raw_off: u64,
    raw_size: u32,
    n_groups: u32,
    group_off: u64,
    group_size: u32,
}

impl Disc {
    fn parse(d: &[u8]) -> Option<Disc> {
        let d = d.get(..DISC_LEN)?;
        Some(Disc {
            disc_type: be32(d, 0),
            compression: be32(d, 4),
            chunk: u64::from(be32(d, 12)),
            dhead: d[16..0x90].try_into().ok()?,
            n_raw: be32(d, 0xB4),
            raw_off: be64(d, 0xB8),
            raw_size: be32(d, 0xC0),
            n_groups: be32(d, 0xC4),
            group_off: be64(d, 0xC8),
            group_size: be32(d, 0xD0),
        })
    }
}

/// A `wia_raw_data_t`.
#[derive(Debug, Clone, Copy)]
struct Raw {
    off: u64,
    size: u64,
    first_group: u32,
    groups: u32,
}

/// An `rvz_group_t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Group {
    off4: u32,
    size: u32,
    packed: u32,
}

impl Group {
    const COMPRESSED: u32 = 0x8000_0000;
}

/// Decode RVZ packing: `data` (the group's bytes after decompression) at
/// `disc_off` on the disc, into `out`.
fn unpack(data: &[u8], mut disc_off: u64, out: &mut Vec<u8>) -> Option<()> {
    let mut at = 0;
    while at < data.len() {
        let size = be32(data.get(at..at + 4)?, 0);
        at += 4;
        let len = (size & 0x7FFF_FFFF) as usize;
        if size & 0x8000_0000 != 0 {
            let seed_bytes = data.get(at..at + 68)?;
            at += 68;
            let seed: [u32; junk::SEED_WORDS] = std::array::from_fn(|i| be32(seed_bytes, i * 4));
            out.extend(junk::generate(
                &seed,
                (disc_off % junk::BLOCK as u64) as usize,
                len,
            ));
        } else {
            out.extend_from_slice(data.get(at..at + len)?);
            at += len;
        }
        disc_off += len as u64;
    }
    Some(())
}

#[cfg(test)]
mod tests;
