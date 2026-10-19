use super::{MAX_SIZE, ips_end, le32, number};

/// A PPF's changes: (offset, bytes) in order, and what it says to check.
pub(crate) struct Ppf<'a> {
    pub records: Vec<(u64, &'a [u8])>,
    /// PPF 2 / 3: 1024 bytes the image must hold at `check_at`.
    pub check: Option<(u64, &'a [u8])>,
    /// PPF 2: the image's size.
    pub size: Option<u64>,
    pub description: String,
}

impl Ppf<'_> {
    /// Whether an image (its first bytes, or all of it) is the one this PPF
    /// was made for, as far as it says.
    pub fn fits(&self, image: &[u8]) -> bool {
        self.size.is_none_or(|s| s == image.len() as u64)
            && self
                .check
                .is_none_or(|(at, block)| image.get(at as usize..at as usize + block.len()) == Some(block))
    }
}

/// Read a PPF 1, 2 or 3 (its `FILE_ID.DIZ` at the end is left out).
pub(crate) fn ppf_records(data: &[u8]) -> Option<Ppf<'_>> {
    let version = match data.get(..5)? {
        b"PPF10" => 1,
        b"PPF20" => 2,
        b"PPF30" => 3,
        _ => return None,
    };
    let description = String::from_utf8_lossy(data.get(6..56)?)
        .trim_end_matches(['\0', ' '])
        .to_owned();
    // A FILE_ID.DIZ at the end: "@BEGIN_FILE_ID.DIZ" ... "@END_FILE_ID.DIZ"
    // then its length.
    let end = match data.windows(18).rposition(|w| w == b"@BEGIN_FILE_ID.DIZ") {
        Some(at) if version > 1 => at,
        _ => data.len(),
    };
    // After the header: where the records start, the block to check (its
    // place depends on the image type: BIN 0x9320, GI 0x80A0), the image's
    // size, and whether each record carries undo bytes.
    let (mut at, check, size, undo) = match version {
        1 => (56, None, None, false),
        2 => {
            let size = le32(data.get(56..60)?) as u64;
            (1084, Some((0x9320u64, data.get(60..1084)?)), Some(size), false)
        }
        _ => {
            let gi = *data.get(56)? == 1;
            let block = *data.get(57)? != 0;
            let undo = *data.get(58)? != 0;
            if block {
                let at = if gi { 0x80a0 } else { 0x9320 };
                (1084, Some((at, data.get(60..1084)?)), None, undo)
            } else {
                (60, None, None, undo)
            }
        }
    };
    let mut records = Vec::new();
    while at < end {
        let offset = if version == 3 {
            let b = data.get(at..at + 8)?;
            at += 8;
            u64::from_le_bytes(b.try_into().ok()?)
        } else {
            let b = data.get(at..at + 4)?;
            at += 4;
            le32(b) as u64
        };
        let len = *data.get(at)? as usize;
        at += 1;
        records.push((offset, data.get(at..at + len)?));
        at += len;
        if undo {
            at += len; // the bytes it replaces, for undoing it
        }
    }
    (at == end).then_some(Ppf {
        records,
        check,
        size,
        description,
    })
}

pub(super) fn apply_ips(patch: &[u8], source: &[u8]) -> Option<Vec<u8>> {
    let end = ips_end(patch)?;
    let mut out = source.to_vec();
    let mut at = 5;
    while at < end {
        let offset = u32::from_be_bytes([0, patch[at], patch[at + 1], patch[at + 2]]) as usize;
        let size = u16::from_be_bytes([patch[at + 3], patch[at + 4]]) as usize;
        at += 5;
        let (bytes, run): (&[u8], usize) = if size == 0 {
            // RLE: a 2-byte count, then the byte.
            let count = u16::from_be_bytes([*patch.get(at)?, *patch.get(at + 1)?]) as usize;
            let b = patch.get(at + 2..at + 3)?;
            at += 3;
            (b, count)
        } else {
            let b = patch.get(at..at + size)?;
            at += size;
            (b, size)
        };
        if (offset + run) as u64 > MAX_SIZE {
            return None;
        }
        if out.len() < offset + run {
            out.resize(offset + run, 0);
        }
        if size == 0 {
            out[offset..offset + run].fill(bytes[0]);
        } else {
            out[offset..offset + run].copy_from_slice(bytes);
        }
    }
    // Lunar IPS: a 3-byte size after "EOF" truncates the result.
    if let Some(t) = patch.get(end + 3..end + 6) {
        out.truncate(u32::from_be_bytes([0, t[0], t[1], t[2]]) as usize);
    }
    Some(out)
}

pub(super) fn apply_bps(patch: &[u8], source: &[u8]) -> Option<Vec<u8>> {
    let mut at = 4;
    let _source_size = number(patch, &mut at)?;
    let target_size = number(patch, &mut at)?;
    if target_size > MAX_SIZE {
        return None;
    }
    let metadata = usize::try_from(number(patch, &mut at)?).ok()?;
    at = at.checked_add(metadata)?;
    let end = patch.len() - 12;
    if at > end {
        return None;
    }
    let mut out: Vec<u8> = Vec::with_capacity(target_size as usize);
    let (mut source_rel, mut target_rel) = (0i64, 0i64);
    let signed = |n: u64| -> Option<i64> {
        let v = i64::try_from(n >> 1).ok()?;
        Some(if n & 1 != 0 { -v } else { v })
    };
    while at < end {
        let data = number(patch, &mut at)?;
        let len = usize::try_from(data >> 2).ok()?.checked_add(1)?;
        // Never past the size the patch declares, whatever it asks for (a
        // forged patch's own CRC32 is easy to fake).
        if (out.len() as u64).checked_add(len as u64)? > target_size {
            return None;
        }
        match data & 3 {
            // SourceRead: the source's bytes at the output position.
            0 => {
                let from = out.len();
                out.extend_from_slice(source.get(from..from.checked_add(len)?)?);
            }
            // TargetRead: bytes from the patch.
            1 => {
                out.extend_from_slice(patch.get(at..at.checked_add(len)?)?);
                at += len;
            }
            // SourceCopy: from a moving position in the source.
            2 => {
                source_rel = source_rel.checked_add(signed(number(patch, &mut at)?)?)?;
                let from = usize::try_from(source_rel).ok()?;
                out.extend_from_slice(source.get(from..from.checked_add(len)?)?);
                source_rel = source_rel.checked_add(len as i64)?;
            }
            // TargetCopy: from earlier output, byte by byte (may overlap).
            _ => {
                target_rel = target_rel.checked_add(signed(number(patch, &mut at)?)?)?;
                for _ in 0..len {
                    let b = *out.get(usize::try_from(target_rel).ok()?)?;
                    out.push(b);
                    target_rel += 1;
                }
            }
        }
    }
    Some(out)
}

pub(super) fn apply_ups(patch: &[u8], source: &[u8]) -> Option<Vec<u8>> {
    let mut at = 4;
    let _source_size = number(patch, &mut at)?;
    let target_size = number(patch, &mut at)? as usize;
    if target_size as u64 > MAX_SIZE {
        return None;
    }
    let end = patch.len() - 12;
    let mut out = source.to_vec();
    out.resize(target_size, 0);
    let mut pos = 0usize;
    while at < end {
        pos = pos.checked_add(usize::try_from(number(patch, &mut at)?).ok()?)?;
        // XOR bytes up to a zero byte.
        loop {
            let x = *patch.get(at)?;
            at += 1;
            if x == 0 {
                pos += 1;
                break;
            }
            if let Some(b) = out.get_mut(pos) {
                *b ^= x;
            }
            pos += 1;
        }
    }
    Some(out)
}
