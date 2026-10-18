//! ROM patches: IPS, BPS and UPS (milestone 12.7).
//!
//! BPS and UPS name what they were made from (source size and CRC32) and
//! what they make; IPS says nothing. Applying a patch to a hash-verified
//! copy of its base and comparing the result's SHA1 with a file proves the
//! file is that patch of that game: still never "verified" (house rule 8),
//! but no longer a guess. Patches are read, never written, and nothing is
//! applied to files on disk.

/// The patch formats romforge reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ips,
    Bps,
    Ups,
}

/// File extensions of patches, lower case.
pub const EXTENSIONS: &[&str] = &["bps", "ups", "ips"];

/// What a patch's own header and footer say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Info {
    pub kind: Kind,
    /// BPS / UPS: the source's and target's size and CRC32.
    pub source: Option<(u64, u32)>,
    pub target: Option<(u64, u32)>,
}

/// Biggest source or target romforge applies a patch to (cartridges).
const MAX_SIZE: u64 = 64 << 20;

/// Read a patch's header (and for BPS / UPS its footer, whose own CRC32
/// must check out).
pub fn info(data: &[u8]) -> Option<Info> {
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
        value = value.checked_add((b & 0x7f) as u64 * shift)?;
        if b & 0x80 != 0 {
            return Some(value);
        }
        shift = shift.checked_shl(7)?;
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
    };
    if let Some((size, crc)) = i.target
        && (size != out.len() as u64 || crc != crc32fast::hash(&out))
    {
        return None;
    }
    Some(out)
}

fn apply_ips(patch: &[u8], source: &[u8]) -> Option<Vec<u8>> {
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

fn apply_bps(patch: &[u8], source: &[u8]) -> Option<Vec<u8>> {
    let mut at = 4;
    let _source_size = number(patch, &mut at)?;
    let target_size = number(patch, &mut at)?;
    if target_size > MAX_SIZE {
        return None;
    }
    let metadata = number(patch, &mut at)? as usize;
    at += metadata;
    let end = patch.len() - 12;
    let mut out: Vec<u8> = Vec::with_capacity(target_size as usize);
    let (mut source_rel, mut target_rel) = (0i64, 0i64);
    let signed = |n: u64| {
        if n & 1 != 0 {
            -((n >> 1) as i64)
        } else {
            (n >> 1) as i64
        }
    };
    while at < end {
        let data = number(patch, &mut at)?;
        let len = (data >> 2) as usize + 1;
        match data & 3 {
            // SourceRead: the source's bytes at the output position.
            0 => {
                let from = out.len();
                out.extend_from_slice(source.get(from..from + len)?);
            }
            // TargetRead: bytes from the patch.
            1 => {
                out.extend_from_slice(patch.get(at..at + len)?);
                at += len;
            }
            // SourceCopy: from a moving position in the source.
            2 => {
                source_rel += signed(number(patch, &mut at)?);
                let from = usize::try_from(source_rel).ok()?;
                out.extend_from_slice(source.get(from..from + len)?);
                source_rel += len as i64;
            }
            // TargetCopy: from earlier output, byte by byte (may overlap).
            _ => {
                target_rel += signed(number(patch, &mut at)?);
                for _ in 0..len {
                    let b = *out.get(usize::try_from(target_rel).ok()?)?;
                    out.push(b);
                    target_rel += 1;
                }
            }
        }
        if out.len() as u64 > target_size {
            return None;
        }
    }
    Some(out)
}

fn apply_ups(patch: &[u8], source: &[u8]) -> Option<Vec<u8>> {
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
        pos += number(patch, &mut at)? as usize;
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

/// A plain file, or a zip / 7z member, read into memory (at most
/// `MAX_SIZE` bytes).
pub(crate) fn read_entry(path: &std::path::Path, member: &str) -> Option<Vec<u8>> {
    use std::io::Read;
    if member.is_empty() {
        let len = std::fs::metadata(path).ok()?.len();
        return (len <= MAX_SIZE).then(|| std::fs::read(path).ok()).flatten();
    }
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "zip" => {
            let mut zip =
                zip::ZipArchive::new(std::io::BufReader::new(std::fs::File::open(path).ok()?)).ok()?;
            let mut m = zip.by_name(member).ok()?;
            if m.size() > MAX_SIZE {
                return None;
            }
            let mut out = Vec::with_capacity(m.size() as usize);
            m.read_to_end(&mut out).ok()?;
            Some(out)
        }
        "7z" => {
            let mut a = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty()).ok()?;
            let size = a.archive().files.iter().find(|e| e.name() == member)?.size();
            (size <= MAX_SIZE).then(|| a.read_file(member).ok()).flatten()
        }
        _ => None,
    }
}

/// Patches beside a ROM: files of the same name with a patch extension,
/// next to it on disk or in the same archive.
pub(crate) fn beside(path: &std::path::Path, member: &str) -> Vec<Vec<u8>> {
    let stem_of = |name: &str| -> String {
        let n = name.rsplit('/').next().unwrap_or(name);
        n.rsplit_once('.').map_or(n, |(s, _)| s).to_owned()
    };
    let is_patch = |name: &str| {
        name.rsplit_once('.')
            .is_some_and(|(_, e)| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
    };
    let mut out = Vec::new();
    if member.is_empty() {
        let Some(dir) = path.parent() else { return out };
        let stem = stem_of(&path.to_string_lossy());
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if is_patch(&name)
                && stem_of(&name) == stem
                && let Some(d) = read_entry(&e.path(), "")
            {
                out.push(d);
            }
        }
        return out;
    }
    let stem = stem_of(member);
    let names: Vec<String> = match path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .as_deref()
    {
        Some("zip") => zip::ZipArchive::new(std::io::BufReader::new(match std::fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return out,
        }))
        .map(|z| {
            z.file_names()
                .filter_map(|n| n.ok().map(|n| n.into_owned()))
                .collect()
        })
        .unwrap_or_default(),
        Some("7z") => sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())
            .map(|a| a.archive().files.iter().map(|e| e.name().to_owned()).collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    for n in names.iter().filter(|n| is_patch(n) && stem_of(n) == stem) {
        if let Some(d) = read_entry(path, n) {
            out.push(d);
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn num(mut n: u64, out: &mut Vec<u8>) {
        loop {
            let x = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(0x80 | x);
                return;
            }
            out.push(x);
            n -= 1;
        }
    }

    fn footer(p: &mut Vec<u8>, source: &[u8], target: &[u8]) {
        p.extend_from_slice(&crc32fast::hash(source).to_le_bytes());
        p.extend_from_slice(&crc32fast::hash(target).to_le_bytes());
        let own = crc32fast::hash(p);
        p.extend_from_slice(&own.to_le_bytes());
    }

    /// A BPS that copies the source, then writes `tail` (TargetRead).
    pub(crate) fn bps(source: &[u8], tail: &[u8]) -> Vec<u8> {
        let mut target = source.to_vec();
        target.extend_from_slice(tail);
        let mut p = b"BPS1".to_vec();
        num(source.len() as u64, &mut p);
        num(target.len() as u64, &mut p);
        num(0, &mut p);
        num((source.len() as u64 - 1) << 2, &mut p); // SourceRead
        num(((tail.len() as u64 - 1) << 2) | 1, &mut p);
        p.extend_from_slice(tail);
        footer(&mut p, source, &target);
        p
    }

    /// An IPS that writes `bytes` at `offset`.
    pub(crate) fn ips(offset: u32, bytes: &[u8]) -> Vec<u8> {
        let mut p = b"PATCH".to_vec();
        p.extend_from_slice(&offset.to_be_bytes()[1..]);
        p.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        p.extend_from_slice(bytes);
        p.extend_from_slice(b"EOF");
        p
    }

    #[test]
    fn bps_round_trip() {
        let source: Vec<u8> = (0..5000u32).map(|i| i as u8).collect();
        let p = bps(&source, b"translated!");
        let i = info(&p).unwrap();
        assert_eq!(i.kind, Kind::Bps);
        assert_eq!(i.source, Some((5000, crc32fast::hash(&source))));
        let out = apply(&p, &source).unwrap();
        assert_eq!(&out[5000..], b"translated!");
        assert_eq!(apply(&p, &source[1..]), None, "made from something else");
    }

    #[test]
    fn ips_writes_rle_and_grows() {
        let source = vec![0u8; 16];
        let out = apply(&ips(4, b"HI"), &source).unwrap();
        assert_eq!(&out[4..6], b"HI");
        // RLE record past the end grows the file.
        let mut p = b"PATCH".to_vec();
        p.extend_from_slice(&[0, 0, 20, 0, 0, 0, 3, b'z']);
        p.extend_from_slice(b"EOF");
        let out = apply(&p, &source).unwrap();
        assert_eq!((out.len(), &out[20..23]), (23, &b"zzz"[..]));
        assert_eq!(info(&p).unwrap().kind, Kind::Ips);
        assert_eq!(info(b"not a patch"), None);
    }

    #[test]
    fn ups_xor() {
        let source = b"HELLO WORLD".to_vec();
        let target = b"HELLO THERE".to_vec();
        let mut p = b"UPS1".to_vec();
        num(source.len() as u64, &mut p);
        num(target.len() as u64, &mut p);
        num(6, &mut p); // skip "HELLO "
        for (a, b) in source[6..].iter().zip(&target[6..]) {
            p.push(a ^ b);
        }
        p.push(0);
        footer(&mut p, &source, &target);
        assert_eq!(apply(&p, &source).unwrap(), target);
    }
}
