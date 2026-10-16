//! Streaming CRC32 / MD5 / SHA1 over the raw data and the header-stripped data
//! in one pass. Nothing is buffered beyond a small chunk.

use std::io::{self, Read};

use md5::Md5;
use sha1::{Digest as _, Sha1};

use crate::headers::{self, PREFIX_LEN};

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
    /// Present when a copier header was detected.
    pub stripped: Option<(&'static str, Digest)>,
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
    let mut feed = |chunk: &[u8], raw: &mut Hasher| {
        let start = raw.size;
        raw.update(chunk);
        if let Some((h, s)) = stripped.as_mut() {
            let skip = (h.len as u64).saturating_sub(start).min(chunk.len() as u64) as usize;
            s.update(&chunk[skip..]);
        }
    };

    feed(&buf[..filled], &mut raw);
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        feed(&buf[..n], &mut raw);
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

    #[test]
    fn size_mismatch_is_an_error() {
        assert!(hash_stream(&b"abc"[..], 4).is_err());
    }
}
