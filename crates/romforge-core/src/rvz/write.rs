//! Writing a GameCube disc as RVZ, as Dolphin's defaults make it (zstd
//! level 5, 128 KiB chunks): one raw-data range for the whole disc, a
//! group per chunk. In each chunk the padding between the disc's data
//! ([`super::gc`]) is packed as seeds where [`super::junk`] finds one that
//! regenerates it exactly; everything else is kept as it is. A chunk of
//! zeros is stored as nothing; a chunk zstd doesn't shrink is stored plain.
//! Groups are written first, then the two tables, then the headers with
//! their SHA-1s.

use std::io::{self, Read, Seek, SeekFrom, Write};

use sha1::{Digest, Sha1};

use super::{DISC_LEN, GAMECUBE, Group, HEAD_LEN, MAGIC, PART_LEN, VERSION, VERSION_COMPATIBLE, ZSTD, junk};

const CHUNK: u64 = 128 * 1024;
const LEVEL: i32 = 5;
/// Shorter padding isn't worth a 72-byte seed.
const MIN_PACK: u64 = 128;

/// The padding ranges (start, end) between `data` ranges on a disc of `size`.
fn gaps(data: &[(u64, u64)], size: u64) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut at = 0;
    for &(s, e) in data {
        if s > at {
            out.push((at, s));
        }
        at = at.max(e);
    }
    if at < size {
        out.push((at, size));
    }
    out
}

/// A chunk at `off` packed, when some of its padding packs.
fn pack(off: u64, chunk: &[u8], gaps: &[(u64, u64)]) -> Option<Vec<u8>> {
    let end = off + chunk.len() as u64;
    let first = gaps.partition_point(|g| g.1 <= off);
    let mut runs: Vec<(u64, u64, Option<[u32; junk::SEED_WORDS]>)> = Vec::new();
    for &(gs, ge) in gaps[first..].iter().take_while(|g| g.0 < end) {
        // Each 32 KiB block has its own seed: a gap is tried block by block.
        let mut s = gs.max(off);
        let e = ge.min(end);
        while s < e {
            let block_end = (s / junk::BLOCK as u64 + 1) * junk::BLOCK as u64;
            let piece_end = e.min(block_end);
            if piece_end - s >= MIN_PACK {
                let data = &chunk[(s - off) as usize..(piece_end - off) as usize];
                if let Some(seed) = junk::find_seed(data, (s % junk::BLOCK as u64) as usize) {
                    runs.push((s, piece_end, Some(seed)));
                }
            }
            s = piece_end;
        }
    }
    if runs.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(chunk.len() / 2);
    let mut at = off;
    let plain = |out: &mut Vec<u8>, from: u64, to: u64| {
        if to > from {
            out.extend_from_slice(&((to - from) as u32).to_be_bytes());
            out.extend_from_slice(&chunk[(from - off) as usize..(to - off) as usize]);
        }
    };
    for (s, e, seed) in runs {
        plain(&mut out, at, s);
        if let Some(seed) = seed {
            out.extend_from_slice(&(((e - s) as u32) | 0x8000_0000).to_be_bytes());
            for w in seed {
                out.extend_from_slice(&w.to_be_bytes());
            }
        }
        at = e;
    }
    plain(&mut out, at, end);
    Some(out)
}

/// A chunk's group: 8 bytes (the stored-as flags, the packed size), then
/// the bytes to store; nothing at all for zeros.
fn encode(off: u64, chunk: &[u8], gaps: &[(u64, u64)]) -> io::Result<Vec<u8>> {
    if chunk.iter().all(|&b| b == 0) {
        return Ok(Vec::new());
    }
    let packed = pack(off, chunk, gaps);
    let body = packed.as_deref().unwrap_or(chunk);
    let z = crate::chunks::zstd(body, LEVEL)?;
    let (flags, data) = if z.len() < body.len() {
        (Group::COMPRESSED, z)
    } else {
        (0, body.to_vec())
    };
    let mut out = Vec::with_capacity(8 + data.len());
    out.extend_from_slice(&flags.to_be_bytes());
    out.extend_from_slice(&(packed.map_or(0, |p| p.len()) as u32).to_be_bytes());
    out.extend_from_slice(&data);
    Ok(out)
}

/// Write the GameCube disc `open` reads (`size` bytes, opened three times:
/// for its layout, its data, and its first 0x80 bytes) as RVZ into `out`,
/// from its start.
pub fn write<S: Read>(
    open: impl Fn() -> io::Result<S>,
    size: u64,
    threads: usize,
    out: &mut (impl Write + Seek),
) -> io::Result<()> {
    let data = super::gc::data_ranges(&mut open()?, size)?;
    let gaps = gaps(&data, size);
    // Every offset an RVZ stores counts from its file's start.
    out.seek(SeekFrom::Start(0))?;
    out.write_all(&[0u8; HEAD_LEN + DISC_LEN])?;
    let mut at = (HEAD_LEN + DISC_LEN) as u64;
    let mut groups: Vec<Group> = Vec::new();
    let mut dhead = [0u8; 0x80];
    let mut read = 0u64;
    let mut src = open()?;
    crate::chunks::in_order(
        &mut src,
        CHUNK as usize,
        threads,
        |i, c| encode(i * CHUNK, c, &gaps),
        |enc, n| {
            read += n as u64;
            if enc.is_empty() {
                groups.push(Group {
                    off4: 0,
                    size: 0,
                    packed: 0,
                });
                return Ok(());
            }
            let flags = u32::from_be_bytes(enc[..4].try_into().unwrap_or_default());
            let packed = u32::from_be_bytes(enc[4..8].try_into().unwrap_or_default());
            let body = &enc[8..];
            groups.push(Group {
                off4: u32::try_from(at / 4).map_err(|_| io::Error::other("RVZ: the file passes 16 GiB"))?,
                size: body.len() as u32 | flags,
                packed,
            });
            out.write_all(body)?;
            at += body.len() as u64;
            let pad = (4 - at % 4) % 4;
            out.write_all(&[0u8; 3][..pad as usize])?;
            at += pad;
            Ok(())
        },
    )?;
    if read != size {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("RVZ: read {read} bytes of {size}"),
        ));
    }
    // The disc's first 0x80 bytes, read again from the start.
    crate::chunks::read_full(&mut open()?, &mut dhead)?;
    let n_groups = groups.len() as u32;
    let mut raw = Vec::with_capacity(super::RAW_LEN);
    raw.extend_from_slice(&0x80u64.to_be_bytes());
    raw.extend_from_slice(&(size - 0x80).to_be_bytes());
    raw.extend_from_slice(&0u32.to_be_bytes());
    raw.extend_from_slice(&n_groups.to_be_bytes());
    let raw = crate::chunks::zstd(&raw, LEVEL)?;
    let raw_off = at;
    out.write_all(&raw)?;
    at += raw.len() as u64;
    let mut table = Vec::with_capacity(groups.len() * super::GROUP_LEN);
    for g in &groups {
        for v in [g.off4, g.size, g.packed] {
            table.extend_from_slice(&v.to_be_bytes());
        }
    }
    let table = crate::chunks::zstd(&table, LEVEL)?;
    let group_off = at;
    out.write_all(&table)?;
    at += table.len() as u64;

    let mut disc = Vec::with_capacity(DISC_LEN);
    for v in [GAMECUBE, ZSTD, LEVEL as u32, CHUNK as u32] {
        disc.extend_from_slice(&v.to_be_bytes());
    }
    disc.extend_from_slice(&dhead);
    disc.extend_from_slice(&0u32.to_be_bytes());
    disc.extend_from_slice(&PART_LEN.to_be_bytes());
    disc.extend_from_slice(&((HEAD_LEN + DISC_LEN) as u64).to_be_bytes());
    disc.extend_from_slice(&Sha1::digest([]));
    disc.extend_from_slice(&1u32.to_be_bytes());
    disc.extend_from_slice(&raw_off.to_be_bytes());
    disc.extend_from_slice(&(raw.len() as u32).to_be_bytes());
    disc.extend_from_slice(&n_groups.to_be_bytes());
    disc.extend_from_slice(&group_off.to_be_bytes());
    disc.extend_from_slice(&(table.len() as u32).to_be_bytes());
    disc.extend_from_slice(&[0u8; 8]);
    let mut head = Vec::with_capacity(HEAD_LEN);
    head.extend_from_slice(&MAGIC);
    head.extend_from_slice(&VERSION.to_be_bytes());
    head.extend_from_slice(&VERSION_COMPATIBLE.to_be_bytes());
    head.extend_from_slice(&(DISC_LEN as u32).to_be_bytes());
    head.extend_from_slice(&Sha1::digest(&disc));
    head.extend_from_slice(&size.to_be_bytes());
    head.extend_from_slice(&at.to_be_bytes());
    let own = Sha1::digest(&head);
    head.extend_from_slice(&own);
    out.seek(SeekFrom::Start(0))?;
    out.write_all(&head)?;
    out.write_all(&disc)?;
    out.seek(SeekFrom::Start(at))?;
    Ok(())
}
