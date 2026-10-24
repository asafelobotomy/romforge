//! Where a GameCube disc's data is, from its own header (big-endian, as
//! the disc stores it): the boot block and `bi2.bin` (0 to 0x2440), the
//! apploader (0x2440: a 0x20-byte header with its size at 0x14 and its
//! trailer's size at 0x18), the main DOL (offset at 0x420; its size the end
//! of its furthest section, from 18 section offsets at 0 and sizes at
//! 0x90), the FST (offset at 0x424, size at 0x428) and every file the FST
//! lists (12-byte entries: a file is type 0 with its offset and length).
//! Everything else is padding, which RVZ can pack.

use std::io::{self, Read};

/// The (start, end) ranges of a disc of `size` bytes that hold data,
/// sorted and merged,
/// read from `disc` (a stream of the image from its start), which is read
/// only as far as the FST and DOL headers.
pub fn data_ranges(disc: &mut impl Read, size: u64) -> io::Result<Vec<(u64, u64)>> {
    let mut head = vec![0u8; 0x2460];
    if crate::chunks::read_full(disc, &mut head)? < head.len() {
        return Ok(vec![(0, size)]);
    }
    let be =
        |b: &[u8], at: usize| u64::from(u32::from_be_bytes(b[at..at + 4].try_into().unwrap_or_default()));
    let (dol, fst, fst_len) = (be(&head, 0x420), be(&head, 0x424), be(&head, 0x428));
    let apploader = 0x20 + be(&head, 0x2454) + be(&head, 0x2458);
    // Header values are untrusted: a layout that doesn't fit the disc is
    // all data (nothing is packed).
    if dol < 0x2460 || fst < 0x2460 || fst + fst_len > size || fst_len > 64 << 20 || dol + 0x100 > size {
        return Ok(vec![(0, size)]);
    }
    let mut wanted = [(dol, 0x100u64), (fst, fst_len)];
    wanted.sort();
    let mut got: Vec<Vec<u8>> = Vec::new();
    let mut pos = head.len() as u64;
    for (at, len) in wanted {
        if at < pos {
            return Ok(vec![(0, size)]); // overlapping or before the header's end
        }
        io::copy(&mut disc.take(at - pos), &mut io::sink())?;
        let mut buf = vec![0u8; len as usize];
        if crate::chunks::read_full(disc, &mut buf)? < buf.len() {
            return Ok(vec![(0, size)]);
        }
        pos = at + len;
        got.push(buf);
    }
    let (dol_head, fst_data) = if dol < fst {
        (&got[0], &got[1])
    } else {
        (&got[1], &got[0])
    };
    let dol_len = (0..18)
        .map(|i| be(dol_head, i * 4) + be(dol_head, 0x90 + i * 4))
        .max()
        .unwrap_or(0)
        .max(0x100);
    let mut ranges = vec![(0, 0x2440 + apploader), (dol, dol_len), (fst, fst_len)];
    let entries = if fst_data.len() >= 12 {
        be(fst_data, 8) as usize
    } else {
        0
    };
    for e in fst_data.as_chunks::<12>().0.iter().take(entries).skip(1) {
        if e[0] == 0 {
            let (at, len) = (be(e, 4), be(e, 8));
            if len > 0 && at + len <= size {
                ranges.push((at, len));
            }
        }
    }
    ranges.sort();
    let mut merged: Vec<(u64, u64)> = Vec::new();
    for (at, len) in ranges {
        let end = (at + len).min(size);
        match merged.last_mut() {
            Some((_, e)) if at <= *e => *e = (*e).max(end),
            _ => merged.push((at, end)),
        }
    }
    Ok(merged)
}
