//! Milestone 25: WUA, Cemu's Wii U archive (ZArchive, MIT-0), read in
//! place: its footer, names and file tree, and only the 64 KiB blocks a
//! small file needs (`ruzstd`). Laid out as ZArchive's
//! `zarchivecommon.h` and `zarchivereader.cpp` write and read it,
//! big-endian:
//! - the footer, at the very end: six (offset, size) pairs (compressed
//!   data, offset records, names, file tree, meta directory, metadata), a
//!   32-byte hash, the total size, the version and the magic;
//! - the file tree: 16-byte entries, the root first; a name offset's top
//!   bit marks a file (48-bit offset and size in the uncompressed data), a
//!   directory has its first child's index and a count;
//! - names: a length byte (bit 7 set: 15 bits over two bytes), then the
//!   name;
//! - data: 64 KiB blocks, each zstd unless stored whole; an offset record
//!   has the first of 16 blocks' offset and each one's size minus one.
//!
//! Nothing is unpacked to disk (house rule 7).

use std::io::{Read, Seek, SeekFrom};

const MAGIC: u32 = 0x169f_52d6;
const VERSION: u32 = 0x61bf_3a01;
const FOOTER_LEN: u64 = 6 * 16 + 32 + 8 + 4 + 4;
const BLOCK: u64 = 64 * 1024;
const PER_RECORD: u64 = 16;
const RECORD_LEN: usize = 8 + 2 * 16;
/// The most of one file read back (a `meta.xml` is a few KiB).
const MAX_READ: u64 = 1 << 20;

/// One file or folder of the tree.
#[derive(Debug, Clone, Copy)]
struct Node {
    name: u32,
    file: bool,
    /// A file's offset and size, or a folder's first child and count.
    a: u64,
    b: u64,
}

/// An open WUA: its tree and where its data is.
pub struct Archive<R> {
    file: R,
    names: Vec<u8>,
    tree: Vec<Node>,
    records: Vec<u8>,
    data: (u64, u64),
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes(b.try_into().unwrap_or_default())
}

fn be64(b: &[u8]) -> u64 {
    u64::from_be_bytes(b.try_into().unwrap_or_default())
}

fn read_at<R: Read + Seek>(f: &mut R, at: u64, len: u64) -> Option<Vec<u8>> {
    f.seek(SeekFrom::Start(at)).ok()?;
    let mut buf = vec![0u8; usize::try_from(len).ok()?];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

impl<R: Read + Seek> Archive<R> {
    /// `file` as a WUA, or `None` when it isn't one (or is cut short).
    pub fn open(mut file: R) -> Option<Archive<R>> {
        let size = file.seek(SeekFrom::End(0)).ok()?;
        let foot = read_at(&mut file, size.checked_sub(FOOTER_LEN)?, FOOTER_LEN)?;
        let tail = FOOTER_LEN as usize;
        if be32(&foot[tail - 4..]) != MAGIC || be32(&foot[tail - 8..tail - 4]) != VERSION {
            return None;
        }
        if be64(&foot[tail - 16..tail - 8]) != size {
            return None;
        }
        let section = |i: usize| -> Option<(u64, u64)> {
            let (at, len) = (
                be64(&foot[i * 16..i * 16 + 8]),
                be64(&foot[i * 16 + 8..i * 16 + 16]),
            );
            (at.checked_add(len)? <= size).then_some((at, len))
        };
        let (data, records, names, tree) = (section(0)?, section(1)?, section(2)?, section(3)?);
        // Header values are untrusted: no section bigger than a tree of
        // millions of files would need.
        if records.1 > 1 << 30 || names.1 > 1 << 28 || tree.1 > 1 << 28 {
            return None;
        }
        let records = read_at(&mut file, records.0, records.1)?;
        let names = read_at(&mut file, names.0, names.1)?;
        let raw = read_at(&mut file, tree.0, tree.1)?;
        let tree: Vec<Node> = raw
            .as_chunks::<16>()
            .0
            .iter()
            .map(|e| {
                let (n, x, y, z) = (be32(&e[..4]), be32(&e[4..8]), be32(&e[8..12]), be32(&e[12..]));
                let file = n & 0x8000_0000 != 0;
                let (a, b) = if file {
                    (
                        u64::from(x) | (u64::from(z & 0xFFFF) << 32),
                        u64::from(y) | (u64::from(z & 0xFFFF_0000) << 16),
                    )
                } else {
                    (u64::from(x), u64::from(y))
                };
                Node {
                    name: n & 0x7FFF_FFFF,
                    file,
                    a,
                    b,
                }
            })
            .collect();
        let root = tree.first()?;
        if root.file {
            return None;
        }
        let archive = Archive {
            file,
            names,
            tree,
            records,
            data,
        };
        archive.name(0).is_some_and(str::is_empty).then_some(archive)
    }

    fn name(&self, i: usize) -> Option<&str> {
        let at = self.tree.get(i)?.name as usize;
        let b = *self.names.get(at)?;
        let (len, from) = if b & 0x80 != 0 {
            let hi = *self.names.get(at + 1)? as usize;
            (usize::from(b & 0x7F) | (hi << 7), at + 2)
        } else {
            (usize::from(b), at + 1)
        };
        std::str::from_utf8(self.names.get(from..from + len)?).ok()
    }

    /// The children of folder `i`: (index, name, is a file).
    fn children(&self, i: usize) -> Vec<(usize, &str, bool)> {
        let Some(n) = self.tree.get(i).filter(|n| !n.file) else {
            return Vec::new();
        };
        let (first, count) = (n.a as usize, n.b as usize);
        (first..first.saturating_add(count).min(self.tree.len()))
            .filter_map(|c| Some((c, self.name(c)?, self.tree[c].file)))
            .collect()
    }

    /// The folders at the top (Wii U: `<title ID>_v<version>`).
    pub fn top_folders(&self) -> Vec<String> {
        self.children(0)
            .into_iter()
            .filter(|(_, _, file)| !file)
            .map(|(_, n, _)| n.to_owned())
            .collect()
    }

    /// The file at `path` (`/`-separated, names compared without case, as
    /// ZArchive does), read whole when it is at most 1 MiB.
    pub fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        let mut at = 0;
        for part in path.split('/').filter(|p| !p.is_empty()) {
            at = self
                .children(at)
                .into_iter()
                .find(|(_, n, _)| n.eq_ignore_ascii_case(part))?
                .0;
        }
        let node = *self.tree.get(at).filter(|n| n.file)?;
        if node.b > MAX_READ {
            return None;
        }
        let mut out = Vec::with_capacity(node.b as usize);
        let (start, end) = (node.a, node.a.checked_add(node.b)?);
        let mut pos = start;
        while pos < end {
            let block = self.block(pos / BLOCK)?;
            let from = (pos % BLOCK) as usize;
            let take = ((end - pos) as usize).min(block.len().saturating_sub(from));
            if take == 0 {
                return None;
            }
            out.extend_from_slice(&block[from..from + take]);
            pos += take as u64;
        }
        Some(out)
    }

    /// Block `index`, uncompressed.
    fn block(&mut self, index: u64) -> Option<Vec<u8>> {
        let rec = usize::try_from(index / PER_RECORD).ok()?;
        let sub = (index % PER_RECORD) as usize;
        let r = self.records.get(rec * RECORD_LEN..(rec + 1) * RECORD_LEN)?;
        let size_of = |i: usize| u64::from(u16::from_be_bytes([r[8 + 2 * i], r[9 + 2 * i]])) + 1;
        let offset = be64(&r[..8]) + (0..sub).map(size_of).sum::<u64>();
        let stored = size_of(sub);
        if offset.checked_add(stored)? > self.data.1 {
            return None;
        }
        let raw = read_at(&mut self.file, self.data.0 + offset, stored)?;
        if stored == BLOCK {
            return Some(raw);
        }
        let mut out = Vec::with_capacity(BLOCK as usize);
        let decoder = ruzstd::decoding::StreamingDecoder::new(&raw[..]).ok()?;
        decoder.take(BLOCK).read_to_end(&mut out).ok()?;
        Some(out)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A WUA holding `files` (path, bytes), in one raw zstd block, laid out
    /// as ZArchive writes it.
    pub(crate) fn wua_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        // The tree, breadth first: folders' children are contiguous.
        #[derive(Default)]
        struct Dir {
            dirs: Vec<(String, Dir)>,
            files: Vec<(String, u64, u64)>,
        }
        let mut root = Dir::default();
        let mut data = Vec::new();
        for (path, bytes) in files {
            let parts: Vec<&str> = path.split('/').collect();
            let mut d = &mut root;
            for p in &parts[..parts.len() - 1] {
                let i = match d.dirs.iter().position(|(n, _)| n == p) {
                    Some(i) => i,
                    None => {
                        d.dirs.push(((*p).to_owned(), Dir::default()));
                        d.dirs.len() - 1
                    }
                };
                d = &mut d.dirs[i].1;
            }
            d.files.push((
                parts[parts.len() - 1].to_owned(),
                data.len() as u64,
                bytes.len() as u64,
            ));
            data.extend_from_slice(bytes);
        }
        let mut names = Vec::new();
        let mut name = |n: &str| {
            let at = names.len() as u32;
            names.push(n.len() as u8);
            names.extend_from_slice(n.as_bytes());
            at
        };
        let root_name = name("");
        let mut tree: Vec<[u32; 4]> = vec![[root_name, 0, 0, 0]];
        let mut queue: Vec<(usize, &Dir)> = vec![(0, &root)];
        while let Some((at, d)) = queue.pop() {
            let first = tree.len() as u32;
            tree[at][1] = first;
            tree[at][2] = (d.dirs.len() + d.files.len()) as u32;
            for (n, sub) in &d.dirs {
                queue.push((tree.len(), sub));
                tree.push([name(n), 0, 0, 0]);
            }
            for (n, off, len) in &d.files {
                tree.push([name(n) | 0x8000_0000, *off as u32, *len as u32, 0]);
            }
        }
        // One 64 KiB block (data under 64 KiB, padded with zeros as
        // ZArchive does) as one zstd frame: the data raw, the padding as
        // a run-length block.
        let mut frame = vec![0x28, 0xB5, 0x2F, 0xFD, 0xE0];
        frame.extend_from_slice(&BLOCK.to_le_bytes());
        let raw = ((data.len() as u32) << 3) & !1;
        frame.extend_from_slice(&raw.to_le_bytes()[..3]);
        frame.extend_from_slice(&data);
        let rle = (((BLOCK as u32) - data.len() as u32) << 3) | (1 << 1) | 1;
        frame.extend_from_slice(&rle.to_le_bytes()[..3]);
        frame.push(0);
        let mut out = frame.clone();
        let mut record = 0u64.to_be_bytes().to_vec();
        record.extend_from_slice(&((frame.len() - 1) as u16).to_be_bytes());
        record.resize(RECORD_LEN, 0);
        let mut sections = vec![(0u64, frame.len() as u64)];
        for part in [
            record,
            names.clone(),
            tree.iter()
                .flat_map(|e| e.iter().flat_map(|w| w.to_be_bytes()))
                .collect(),
        ] {
            sections.push((out.len() as u64, part.len() as u64));
            out.extend_from_slice(&part);
        }
        sections.extend([(out.len() as u64, 0), (out.len() as u64, 0)]);
        for (at, len) in sections {
            out.extend_from_slice(&at.to_be_bytes());
            out.extend_from_slice(&len.to_be_bytes());
        }
        out.extend_from_slice(&[0; 32]);
        let total = out.len() as u64 + 16;
        out.extend_from_slice(&total.to_be_bytes());
        out.extend_from_slice(&VERSION.to_be_bytes());
        out.extend_from_slice(&MAGIC.to_be_bytes());
        out
    }

    #[test]
    fn a_wua_s_files_are_read_in_place() {
        let meta = b"<menu><product_code>WUP-P-ARPE</product_code></menu>";
        let file = wua_of(&[
            ("0005000010101e00_v0/meta/meta.xml", meta),
            ("0005000010101e00_v0/code/app.xml", b"<app/>"),
            ("0005000e10101e00_v16/meta/meta.xml", b"update"),
        ]);
        let mut a = Archive::open(std::io::Cursor::new(&file)).unwrap();
        assert_eq!(a.top_folders(), ["0005000010101e00_v0", "0005000e10101e00_v16"]);
        assert_eq!(a.read("0005000010101e00_v0/META/meta.xml").unwrap(), meta);
        assert_eq!(a.read("0005000e10101e00_v16/meta/meta.xml").unwrap(), b"update");
        assert!(a.read("0005000010101e00_v0/meta/nothing").is_none());
        // Cut short, or not a WUA: nothing, never a panic.
        assert!(Archive::open(std::io::Cursor::new(&file[..file.len() - 3])).is_none());
        assert!(Archive::open(std::io::Cursor::new(b"WUP-P-ARPE".to_vec())).is_none());
    }
}
