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

use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
const MAX_SIZE: u64 = 64 << 20;

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
        Kind::Xdelta => oxidelta::vcdiff::decode_memory(patch, source).ok()?,
    };
    if let Some((size, crc)) = i.target
        && (size != out.len() as u64 || crc != crc32fast::hash(&out))
    {
        return None;
    }
    Some(out)
}

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

/// Apply a patch to the file `source`, writing `out` (which must not
/// exist), without holding a disc image in memory: a PPF is copied then
/// changed in place, an xdelta decoded window by window reading the source
/// where it points. The others (cartridge formats) go through [`apply`].
pub fn apply_file(patch: &[u8], source: &Path, out: &Path) -> Result<(), crate::Error> {
    use std::io::{Seek, SeekFrom, Write};
    let bad = |m: &str| crate::Error::Dat(format!("patch: {m}"));
    let i = info(patch).ok_or_else(|| bad("not a patch romforge reads"))?;
    match i.kind {
        Kind::Ppf => {
            let ppf = ppf_records(patch).ok_or_else(|| bad("a damaged PPF"))?;
            let mut f = std::fs::File::open(source)?;
            if let Some((at, block)) = ppf.check {
                let mut have = vec![0u8; block.len()];
                f.seek(SeekFrom::Start(at))?;
                if std::io::Read::read_exact(&mut f, &mut have).is_err() || have != block {
                    return Err(bad(
                        "this PPF was made for another image (its check block differs)",
                    ));
                }
            }
            if ppf
                .size
                .is_some_and(|s| s != f.metadata().map(|m| m.len()).unwrap_or(0))
            {
                return Err(bad("this PPF was made for an image of another size"));
            }
            drop(f);
            let mut w = std::fs::File::create_new(out)?;
            std::io::copy(&mut std::fs::File::open(source)?, &mut w)?;
            for (at, data) in &ppf.records {
                w.seek(SeekFrom::Start(*at))?;
                w.write_all(data)?;
            }
            w.sync_all()?;
            Ok(())
        }
        Kind::Xdelta => {
            let mut src = FileSource(std::fs::File::open(source)?, std::fs::metadata(source)?.len());
            let mut w = std::io::BufWriter::new(std::fs::File::create_new(out)?);
            oxidelta::compress::DeltaDecoder::new(patch)
                .decode_to(&mut src, &mut w)
                .map_err(|e| bad(&format!("xdelta: {e}")))?;
            w.into_inner().map_err(|e| e.into_error())?.sync_all()?;
            Ok(())
        }
        _ => {
            let data = std::fs::read(source)?;
            let patched = apply(patch, &data).ok_or_else(|| bad("it doesn't fit this file"))?;
            std::fs::write(out, patched)?;
            Ok(())
        }
    }
}

/// An xdelta's source, read from a file where the patch points.
struct FileSource(std::fs::File, u64);

impl oxidelta::vcdiff::decoder::SourceProvider for FileSource {
    fn read_source(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, oxidelta::vcdiff::DecodeError> {
        use std::io::{Read, Seek, SeekFrom};
        let io = oxidelta::vcdiff::DecodeError::Io;
        self.0.seek(SeekFrom::Start(offset)).map_err(io)?;
        let mut n = 0;
        while n < buf.len() {
            match self.0.read(&mut buf[n..]).map_err(io)? {
                0 => break,
                k => n += k,
            }
        }
        Ok(n)
    }

    fn source_len(&self) -> Option<u64> {
        Some(self.1)
    }
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
            // The declared size may lie: never read past the limit.
            m.by_ref().take(MAX_SIZE + 1).read_to_end(&mut out).ok()?;
            (out.len() as u64 <= MAX_SIZE).then_some(out)
        }
        "7z" => {
            let mut a = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty()).ok()?;
            let size = a.archive().files.iter().find(|e| e.name() == member)?.size();
            (size <= MAX_SIZE).then(|| a.read_file(member).ok()).flatten()
        }
        _ => None,
    }
}

/// Finds the patches beside ROMs: files of the same name with a patch
/// extension, next to it on disk or in the same archive. Each folder and
/// archive is listed once, however many ROMs are asked about.
#[derive(Default)]
pub(crate) struct PatchFinder {
    /// folder -> its patch files
    dirs: HashMap<PathBuf, Vec<PathBuf>>,
    /// archive -> its patch members
    archives: HashMap<PathBuf, Vec<String>>,
}

fn stem_of(name: &str) -> String {
    let n = name.rsplit('/').next().unwrap_or(name);
    n.rsplit_once('.').map_or(n, |(s, _)| s).to_owned()
}

fn is_patch(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, e)| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

impl PatchFinder {
    /// The patches for the ROM at `path` (or its `member` in that archive).
    pub(crate) fn beside(&mut self, path: &Path, member: &str) -> Vec<Vec<u8>> {
        if member.is_empty() {
            let Some(dir) = path.parent() else {
                return Vec::new();
            };
            let patches = self.dirs.entry(dir.to_path_buf()).or_insert_with(|| {
                std::fs::read_dir(dir)
                    .map(|entries| {
                        entries
                            .flatten()
                            .map(|e| e.path())
                            .filter(|p| p.file_name().is_some_and(|n| is_patch(&n.to_string_lossy())))
                            .collect()
                    })
                    .unwrap_or_default()
            });
            let stem = stem_of(&path.to_string_lossy());
            return patches
                .iter()
                .filter(|p| stem_of(&p.to_string_lossy()) == stem)
                .filter_map(|p| read_entry(p, ""))
                .collect();
        }
        let names = self.archives.entry(path.to_path_buf()).or_insert_with(|| {
            let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
            let all: Vec<String> = match ext.as_deref() {
                Some("zip") => std::fs::File::open(path)
                    .ok()
                    .and_then(|f| zip::ZipArchive::new(std::io::BufReader::new(f)).ok())
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
            all.into_iter().filter(|n| is_patch(n)).collect()
        });
        let stem = stem_of(member);
        let wanted: Vec<String> = names.iter().filter(|n| stem_of(n) == stem).cloned().collect();
        wanted.iter().filter_map(|n| read_entry(path, n)).collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A PPF 3 (no check block, no undo) of `records`.
    pub(crate) fn ppf3(records: &[(u64, &[u8])]) -> Vec<u8> {
        let mut p = b"PPF30".to_vec();
        p.push(2);
        p.extend_from_slice(&[b' '; 50]);
        p.extend_from_slice(&[0, 0, 0, 0]); // BIN, no block, no undo, dummy
        for (at, data) in records {
            p.extend_from_slice(&at.to_le_bytes());
            p.push(data.len() as u8);
            p.extend_from_slice(data);
        }
        p
    }

    #[test]
    fn ppf_versions_apply() {
        let source = vec![0u8; 64];
        let p3 = ppf3(&[(4, b"abc"), (60, b"wxyz")]);
        assert_eq!(info(&p3).map(|i| i.kind), Some(Kind::Ppf));
        let out = apply(&p3, &source).unwrap();
        assert_eq!(&out[4..7], b"abc");
        assert_eq!(out.len(), 64, "60 + 4 stays inside");
        // PPF 1: 32-bit offsets.
        let mut p1 = b"PPF10".to_vec();
        p1.push(0);
        p1.extend_from_slice(&[b' '; 50]);
        p1.extend_from_slice(&10u32.to_le_bytes());
        p1.push(2);
        p1.extend_from_slice(b"hi");
        let out = apply(&p1, &source).unwrap();
        assert_eq!(&out[10..12], b"hi");
        // With a FILE_ID.DIZ at the end (PPF 3).
        let mut with_diz = p3.clone();
        with_diz.extend_from_slice(b"@BEGIN_FILE_ID.DIZhello@END_FILE_ID.DIZ");
        with_diz.extend_from_slice(&5u32.to_le_bytes());
        assert_eq!(apply(&with_diz, &source).unwrap(), apply(&p3, &source).unwrap());
    }

    #[test]
    fn ppf_check_block_refuses_another_image() {
        let mut image = vec![0u8; 0x9320 + 2048];
        image[0x9320..0x9320 + 1024].fill(7);
        let mut p = b"PPF30".to_vec();
        p.push(2);
        p.extend_from_slice(&[b' '; 50]);
        p.extend_from_slice(&[0, 1, 0, 0]); // BIN, a check block
        p.extend_from_slice(&[7u8; 1024]);
        p.extend_from_slice(&5u64.to_le_bytes());
        p.push(1);
        p.push(0xee);
        assert_eq!(apply(&p, &image).unwrap()[5], 0xee);
        image[0x9320] = 8;
        assert!(apply(&p, &image).is_none(), "another image");

        let dir = tempfile::tempdir().unwrap();
        let (src, out) = (dir.path().join("t.bin"), dir.path().join("o.bin"));
        std::fs::write(&src, &image).unwrap();
        let err = apply_file(&p, &src, &out).unwrap_err().to_string();
        assert!(err.contains("another image"), "{err}");
        assert!(!out.exists());
    }

    #[test]
    fn xdelta_in_memory_and_streamed() {
        let source: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let mut target = source.clone();
        target[1000..1010].copy_from_slice(b"TRANSLATED");
        target.extend_from_slice(b"more at the end");
        let mut delta = Vec::new();
        oxidelta::engine::encode(&source, &target, &mut delta).unwrap();
        assert_eq!(info(&delta).map(|i| i.kind), Some(Kind::Xdelta));
        assert_eq!(apply(&delta, &source).unwrap(), target);

        let dir = tempfile::tempdir().unwrap();
        let (src, out) = (dir.path().join("t.bin"), dir.path().join("o.bin"));
        std::fs::write(&src, &source).unwrap();
        apply_file(&delta, &src, &out).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), target);
        let p = ppf3(&[(3, b"zz")]);
        let out2 = dir.path().join("p.bin");
        apply_file(&p, &src, &out2).unwrap();
        assert_eq!(&std::fs::read(&out2).unwrap()[3..5], b"zz");
    }

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

    /// A forged BPS asking for a huge TargetCopy, or claiming absurd
    /// metadata, fails at once instead of filling memory or panicking.
    #[test]
    fn hostile_bps_is_refused() {
        let source = vec![7u8; 16];
        let mut p = b"BPS1".to_vec();
        num(16, &mut p);
        num(32, &mut p);
        num(0, &mut p);
        num(1, &mut p); // TargetRead of 1 byte
        p.push(0xAA);
        num((((1u64 << 40) - 1) << 2) | 3, &mut p); // TargetCopy of 2^40 bytes
        num(0, &mut p);
        p.extend_from_slice(&[0u8; 12]);
        assert_eq!(apply_bps(&p, &source), None);
        let mut q = b"BPS1".to_vec();
        num(16, &mut q);
        num(16, &mut q);
        num(u64::MAX >> 8, &mut q);
        q.extend_from_slice(&[0u8; 12]);
        assert_eq!(apply_bps(&q, &source), None);
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
