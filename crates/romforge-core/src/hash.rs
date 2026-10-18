//! Streaming CRC32 / MD5 / SHA1 over the raw data and the normalized data
//! (header stripped, byte order fixed; see `headers.rs`) in one pass.
//! Nothing is buffered beyond a chunk (16 KiB more for `.smd` blocks).

use std::io::{self, Read};

use md5::Md5;
use sha1::{Digest as _, Sha1};

use crate::headers::{self, PREFIX_LEN};
use crate::ident;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Digest {
    pub size: u64,
    pub crc32: String,
    pub md5: String,
    pub sha1: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hashes {
    pub raw: Digest,
    /// Present when a copier header or another byte order was detected:
    /// the hashes of the data in the DAT's form.
    pub stripped: Option<(&'static str, Digest)>,
    /// What the game's own header or boot files say it is
    /// ([`crate::ident`]), read from the data in the DAT's form.
    pub id: Option<crate::ident::GameId>,
    /// For a cue sheet: the file names its FILE lines give, in order (no
    /// folders), to compare with a game's tracks by content.
    pub sheet: Option<Vec<String>>,
}

/// The FILE names of a small text file that reads as a cue sheet.
fn sheet_of(data: &[u8], size: u64) -> Option<Vec<String>> {
    if size > 64 * 1024 || data.contains(&0) {
        return None;
    }
    let text = String::from_utf8_lossy(data);
    if !text
        .lines()
        .any(|l| l.trim_start().to_ascii_uppercase().starts_with("TRACK "))
    {
        return None;
    }
    let names: Vec<String> = crate::disc::parse_cue(&text)
        .into_iter()
        .map(|f| f.name.rsplit(['/', '\\']).next().unwrap_or(&f.name).to_owned())
        .collect();
    (!names.is_empty()).then_some(names)
}

/// The first [`ident::PREFIX_LEN`] bytes of a stream, for [`ident::read`].
#[derive(Default)]
struct Prefix(Vec<u8>);

impl Prefix {
    fn take(&mut self, data: &[u8]) {
        let room = ident::PREFIX_LEN - self.0.len();
        self.0.extend_from_slice(&data[..room.min(data.len())]);
    }

    fn id(&self) -> Option<crate::ident::GameId> {
        ident::read(&self.0)
    }
}

#[derive(Default)]
struct Hasher {
    size: u64,
    crc: crc32fast::Hasher,
    md5: Md5,
    sha1: Sha1,
}

impl Hasher {
    fn update(&mut self, data: &[u8]) {
        self.size += data.len() as u64;
        self.crc.update(data);
        self.md5.update(data);
        self.sha1.update(data);
    }

    fn finish(self) -> Digest {
        Digest {
            size: self.size,
            crc32: format!("{:08x}", self.crc.finalize()),
            md5: hex(&self.md5.finalize()),
            sha1: hex(&self.sha1.finalize()),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// CRC32 / MD5 / SHA1 fed piece by piece, with no header detection (disc
/// tracks have no copier headers).
#[derive(Default)]
pub struct RawHasher(Hasher, Prefix);

impl RawHasher {
    pub fn update(&mut self, data: &[u8]) {
        self.0.update(data);
        self.1.take(data);
    }

    pub fn finish(self) -> Hashes {
        Hashes {
            id: self.1.id(),
            raw: self.0.finish(),
            stripped: None,
            sheet: None,
        }
    }
}

/// Hash `reader`, whose total length must be `size` (needed up front for the
/// size-based SNES header rule). Errors if the stream length differs.
pub fn hash_stream(mut reader: impl Read, size: u64) -> io::Result<Hashes> {
    let mut buf = vec![0u8; 64 * 1024];

    // Fill at least the header prefix before deciding on a header.
    let mut filled = 0;
    while filled < PREFIX_LEN {
        let n = reader.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    let header = headers::detect(&buf[..filled.min(PREFIX_LEN)], size);

    let mut raw = Hasher::default();
    let mut stripped = header.map(|h| (h, Hasher::default()));
    // Bytes after the header not reordered yet (less than one unit), and
    // the reordered output.
    let (mut pending, mut out): (Vec<u8>, Vec<u8>) = (Vec::new(), Vec::new());
    // The first bytes as they are and in the DAT's form, for the ID
    // readers (the size-only SNES rule may fire on a disc track by chance).
    let (mut raw_prefix, mut prefix) = (Prefix::default(), Prefix::default());
    let mut feed = |chunk: &[u8], raw: &mut Hasher| {
        let start = raw.size;
        raw.update(chunk);
        raw_prefix.take(chunk);
        let Some((h, s)) = stripped.as_mut() else {
            return;
        };
        let skip = (h.len as u64).saturating_sub(start).min(chunk.len() as u64) as usize;
        let body = &chunk[skip..];
        let unit = h.reorder.unit();
        if unit == 1 {
            s.update(body);
            prefix.take(body);
            return;
        }
        pending.extend_from_slice(body);
        let whole = pending.len() / unit * unit;
        h.reorder.apply(&pending[..whole], &mut out);
        s.update(&out);
        prefix.take(&out);
        pending.drain(..whole);
    };

    feed(&buf[..filled], &mut raw);
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        feed(&buf[..n], &mut raw);
    }

    // A trailing piece shorter than one unit is hashed as it is.
    if let Some((_, s)) = stripped.as_mut() {
        s.update(&pending);
        prefix.take(&pending);
    }
    if raw.size != size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected {size} bytes, read {}", raw.size),
        ));
    }
    Ok(Hashes {
        raw: raw.finish(),
        stripped: stripped.map(|(h, s)| (h.kind, s.finish())),
        id: prefix.id().or_else(|| raw_prefix.id()),
        sheet: sheet_of(&raw_prefix.0, size),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reader that returns one byte per call, to exercise chunk boundaries.
    struct Trickle<'a>(&'a [u8]);
    impl Read for Trickle<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() || out.is_empty() {
                return Ok(0);
            }
            out[0] = self.0[0];
            self.0 = &self.0[1..];
            Ok(1)
        }
    }

    #[test]
    fn known_vectors() {
        let h = hash_stream(&b"abc"[..], 3).unwrap();
        assert_eq!(h.raw.crc32, "352441c2");
        assert_eq!(h.raw.md5, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(h.raw.sha1, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert!(h.stripped.is_none());
    }

    #[test]
    fn stripped_equals_hash_of_body() {
        let body: Vec<u8> = (0..1024u32).map(|i| (i * 7) as u8).collect();
        let mut file = b"NES\x1a".to_vec();
        file.resize(16, 0);
        file.extend_from_slice(&body);

        let headered = hash_stream(Trickle(&file), file.len() as u64).unwrap();
        let plain = hash_stream(&body[..], body.len() as u64).unwrap();
        let (kind, stripped) = headered.stripped.unwrap();
        assert_eq!(kind, "ines");
        assert_eq!(stripped, plain.raw);
        assert_ne!(headered.raw, plain.raw);
    }

    /// `.v64`, `.n64` and `.smd` dumps hash as the DAT's form, also when
    /// read a byte at a time and in big chunks.
    #[test]
    fn other_byte_orders_hash_as_the_dats() {
        let mut z64: Vec<u8> = (0..64 * 1024u32).map(|i| (i * 13 + i / 251) as u8).collect();
        z64[..4].copy_from_slice(&[0x80, 0x37, 0x12, 0x40]);
        let plain = hash_stream(&z64[..], z64.len() as u64).unwrap();
        assert!(plain.stripped.is_none(), "z64 is the DAT's order");
        let v64: Vec<u8> = z64.chunks(2).flat_map(|w| [w[1], w[0]]).collect();
        let n64: Vec<u8> = z64.chunks(4).flat_map(|w| [w[3], w[2], w[1], w[0]]).collect();
        // Super Magic Drive: header, then odd bytes and even bytes per 16 KiB.
        let mut smd = vec![0u8; 512];
        smd[1] = 3;
        smd[8..10].copy_from_slice(&[0xAA, 0xBB]);
        for block in z64.chunks(16 * 1024) {
            smd.extend(block.iter().skip(1).step_by(2));
            smd.extend(block.iter().step_by(2));
        }
        for (file, want) in [(&v64, "n64-v64"), (&n64, "n64-n64"), (&smd, "smd")] {
            for h in [
                hash_stream(Trickle(file), file.len() as u64).unwrap(),
                hash_stream(&file[..], file.len() as u64).unwrap(),
            ] {
                let (kind, d) = h.stripped.unwrap();
                assert_eq!(kind, want);
                assert_eq!(d, plain.raw, "{want}");
            }
        }
    }

    #[test]
    fn size_mismatch_is_an_error() {
        assert!(hash_stream(&b"abc"[..], 4).is_err());
    }
}
