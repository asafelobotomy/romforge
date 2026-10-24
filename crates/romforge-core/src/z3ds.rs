//! Milestone 24: Z3DS, Azahar's compressed 3DS files (`.zcci`, `.zcia`,
//! `.zcxi`, `.z3dsx`), read as the file they hold. Laid out as Azahar
//! 2126.2 writes it (`src/common/file_derived.h`, `.cpp`): a 0x20-byte
//! header (magic `Z3DS`, the held file's own magic, version 1, the header's
//! size, the metadata's size, the compressed and uncompressed sizes), the
//! metadata, then the data as seekable zstd, which is plain zstd frames
//! and a skippable seek table: a streaming decoder reads it straight
//! through (`ruzstd`, MIT). Nothing is unpacked to disk (house rule 7).

use std::io::{self, Read, Seek, SeekFrom};

pub const MAGIC: [u8; 4] = *b"Z3DS";
const VERSION: u8 = 1;
const HEADER_LEN: usize = 0x20;

/// A Z3DS file's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// The held file's own magic (`NCSD`, `NCCH`, ...).
    pub underlying_magic: [u8; 4],
    pub header_size: u16,
    pub metadata_size: u32,
    pub compressed_size: u64,
    /// The held file's size: what it matches a DAT's ROM by.
    pub uncompressed_size: u64,
}

impl Header {
    /// Where the compressed data starts.
    fn data_offset(&self) -> u64 {
        u64::from(self.header_size) + u64::from(self.metadata_size)
    }
}

/// The header of a Z3DS file starting at `bytes`, or `None` for another file.
pub fn parse(bytes: &[u8]) -> Option<Header> {
    let b = bytes.get(..HEADER_LEN)?;
    if b[..4] != MAGIC || b[8] != VERSION {
        return None;
    }
    let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap_or_default());
    let u64_at = |i: usize| u64::from_le_bytes(b[i..i + 8].try_into().unwrap_or_default());
    let header = Header {
        underlying_magic: [b[4], b[5], b[6], b[7]],
        header_size: u16_at(10),
        metadata_size: u32_at(12),
        compressed_size: u64_at(16),
        uncompressed_size: u64_at(24),
    };
    (usize::from(header.header_size) >= HEADER_LEN).then_some(header)
}

/// The held file of a Z3DS `file`, as a stream of exactly its size (`None`
/// when `file` isn't one; it is then rewound). A stream that ends early is
/// an error.
pub fn open<R: Read + Seek>(mut file: R) -> io::Result<Option<(Header, impl Read)>> {
    let mut head = [0u8; HEADER_LEN];
    let start = file.stream_position()?;
    let n = read_up_to(&mut file, &mut head)?;
    let Some(header) = parse(&head[..n]) else {
        file.seek(SeekFrom::Start(start))?;
        return Ok(None);
    };
    file.seek(SeekFrom::Start(start + header.data_offset()))?;
    let decoder = ruzstd::decoding::StreamingDecoder::new(file.take(header.compressed_size))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Z3DS: {e}")))?;
    Ok(Some((
        header,
        Exact {
            inner: decoder,
            left: header.uncompressed_size,
        },
    )))
}

fn read_up_to(r: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// Exactly `left` bytes of `inner`: fewer is an error, more are not read.
struct Exact<R> {
    inner: R,
    left: u64,
}

impl<R: Read> Read for Exact<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Ok(0);
        }
        let want = buf.len().min(usize::try_from(self.left).unwrap_or(usize::MAX));
        let n = self.inner.read(&mut buf[..want])?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("Z3DS: the data ends {} bytes short", self.left),
            ));
        }
        self.left -= n as u64;
        Ok(n)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A Z3DS file holding `data` (one raw zstd block: no compression), as
    /// Azahar lays it out, with `meta` bytes of metadata.
    pub(crate) fn z3ds_of(data: &[u8], magic: [u8; 4], meta: usize) -> Vec<u8> {
        // zstd frame: magic, a descriptor with the content size (8 bytes,
        // single segment), then one last raw block.
        let mut frame = vec![0x28, 0xB5, 0x2F, 0xFD, 0xE0];
        frame.extend_from_slice(&(data.len() as u64).to_le_bytes());
        let block = ((data.len() as u32) << 3) | 1; // last, raw
        frame.extend_from_slice(&block.to_le_bytes()[..3]);
        frame.extend_from_slice(data);
        let mut out = Vec::new();
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&magic);
        out.extend_from_slice(&[VERSION, 0]);
        out.extend_from_slice(&(HEADER_LEN as u16).to_le_bytes());
        out.extend_from_slice(&(meta as u32).to_le_bytes());
        out.extend_from_slice(&(frame.len() as u64).to_le_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend(std::iter::repeat_n(0xAB, meta));
        out.extend_from_slice(&frame);
        out
    }

    #[test]
    fn a_z3ds_file_reads_as_the_file_it_holds() {
        let data: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let file = z3ds_of(&data, *b"NCSD", 7);
        let (h, mut r) = open(io::Cursor::new(&file)).unwrap().unwrap();
        assert_eq!(&h.underlying_magic, b"NCSD");
        assert_eq!(h.uncompressed_size, 5000);
        let mut back = Vec::new();
        r.read_to_end(&mut back).unwrap();
        assert_eq!(back, data);
        // Another file is left as it was.
        let mut plain = io::Cursor::new(b"NCSD and the rest".to_vec());
        assert!(open(&mut plain).unwrap().is_none());
        assert_eq!(plain.position(), 0);
    }

    #[test]
    fn a_cut_short_z3ds_file_is_an_error() {
        let data = vec![7u8; 3000];
        let mut file = z3ds_of(&data, *b"NCCH", 0);
        // Claim more than the frame holds.
        file[24..32].copy_from_slice(&4000u64.to_le_bytes());
        let (_, mut r) = open(io::Cursor::new(&file)).unwrap().unwrap();
        let mut back = Vec::new();
        assert!(r.read_to_end(&mut back).is_err());
    }
}
