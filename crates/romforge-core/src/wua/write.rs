//! Writing a WUA as ZArchive's `zarchivewriter.cpp` (MIT-0) does: the
//! files' bytes one after another in 64 KiB blocks (the last padded with
//! zeros), each zstd at level 6 or stored whole when that saves nothing,
//! the data padded to 8 bytes; then the offset records (one per 16
//! blocks), the names (each once), the file tree (breadth first, each
//! folder's children sorted as ZArchive compares names, so a reader may
//! search them), and the footer, whose SHA-256 covers everything before
//! it and the footer itself with that hash zeroed. Checked against
//! ZArchive's own tool both ways (2026-10-24): it extracts romforge's WUAs
//! byte for byte, and romforge reads its WUAs.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use super::{BLOCK, MAGIC, PER_RECORD, VERSION};

const LEVEL: i32 = 6;

/// The longest name a WUA may hold: ZArchive's reader (Cemu's too) misreads
/// the two-byte length of a name of 128 bytes or more, so such an archive
/// couldn't be read back.
pub const MAX_NAME: usize = 0x7F;

/// ZArchive's order of names: case-insensitive (ASCII), shorter first on a
/// common prefix.
fn name_order(a: &str, b: &str) -> std::cmp::Ordering {
    let lower = |s: &str| s.bytes().map(|c| c.to_ascii_lowercase()).collect::<Vec<u8>>();
    lower(a).cmp(&lower(b))
}

#[derive(Default)]
struct Dir {
    dirs: BTreeMap<String, Dir>,
    /// name → (offset, size) in the uncompressed data.
    files: BTreeMap<String, (u64, u64)>,
}

/// Counts and hashes what is written.
struct Out<'a, W: Write> {
    w: &'a mut W,
    at: u64,
    sha: Sha256,
}

impl<W: Write> Out<'_, W> {
    fn put(&mut self, b: &[u8]) -> io::Result<()> {
        self.w.write_all(b)?;
        self.sha.update(b);
        self.at += b.len() as u64;
        Ok(())
    }
}

/// Files read one after another, as one stream.
struct Concat<'a> {
    files: std::slice::Iter<'a, (String, PathBuf)>,
    current: Option<std::fs::File>,
    pad: io::Take<io::Repeat>,
}

impl Read for Concat<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if let Some(f) = &mut self.current {
                match f.read(buf)? {
                    0 => self.current = None,
                    n => return Ok(n),
                }
            }
            match self.files.next() {
                Some((_, path)) => self.current = Some(std::fs::File::open(path)?),
                None => return self.pad.read(buf),
            }
        }
    }
}

/// Write `files` (path in the archive, `/`-separated; file on disk) as a
/// WUA into `out`, compressing on `threads` threads. A file that changes
/// size while it is read is an error.
pub fn write(files: &[(String, PathBuf)], threads: usize, out: &mut impl Write) -> io::Result<()> {
    let mut root = Dir::default();
    let mut offset = 0u64;
    for (path, src) in files {
        let size = std::fs::metadata(src)?.len();
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        if let Some(long) = parts.iter().find(|p| p.len() > MAX_NAME) {
            return Err(io::Error::other(format!(
                "WUA: the name {long:?} is longer than ZArchive's readers take ({MAX_NAME} bytes)"
            )));
        }
        let Some((name, dirs)) = parts.split_last() else {
            return Err(io::Error::other(format!(
                "WUA: an empty path for {}",
                src.display()
            )));
        };
        let mut d = &mut root;
        for p in dirs {
            d = d.dirs.entry((*p).to_owned()).or_default();
        }
        d.files.insert((*name).to_owned(), (offset, size));
        offset += size;
    }
    let pad = (BLOCK - offset % BLOCK) % BLOCK;
    let mut o = Out {
        w: out,
        at: 0,
        sha: Sha256::new(),
    };
    // The data, block by block, with an offset record per 16 blocks.
    let mut records: Vec<(u64, Vec<u16>)> = Vec::new();
    let mut src = Concat {
        files: files.iter(),
        current: None,
        pad: io::repeat(0).take(pad),
    };
    let mut plain = 0u64;
    crate::chunks::in_order(
        &mut src,
        BLOCK as usize,
        threads,
        |_, c| {
            let z = crate::chunks::zstd(c, LEVEL)?;
            Ok(if z.len() as u64 >= BLOCK { c.to_vec() } else { z })
        },
        |stored, n| {
            if records.last().is_none_or(|(_, s)| s.len() as u64 == PER_RECORD) {
                records.push((o.at, Vec::new()));
            }
            if let Some((_, sizes)) = records.last_mut() {
                sizes.push((stored.len() - 1) as u16);
            }
            plain += n as u64;
            o.put(&stored)
        },
    )?;
    if plain != offset + pad {
        return Err(io::Error::other("WUA: a file changed while it was read"));
    }
    let data = (0u64, o.at);
    while !o.at.is_multiple_of(8) {
        o.put(&[0])?;
    }
    let section = |o: &mut Out<_>, bytes: &[u8]| -> io::Result<(u64, u64)> {
        let at = o.at;
        o.put(bytes)?;
        Ok((at, bytes.len() as u64))
    };
    let mut rec = Vec::with_capacity(records.len() * 40);
    for (base, sizes) in &records {
        rec.extend_from_slice(&base.to_be_bytes());
        for i in 0..PER_RECORD as usize {
            rec.extend_from_slice(&sizes.get(i).copied().unwrap_or(0).to_be_bytes());
        }
    }
    let rec = section(&mut o, &rec)?;
    let (names, tree) = layout(&root);
    let names = section(&mut o, &names)?;
    let tree = section(&mut o, &tree)?;
    let meta = (o.at, 0);
    let mut foot = Vec::with_capacity(super::FOOTER_LEN as usize);
    for (at, len) in [data, rec, names, tree, meta, meta] {
        foot.extend_from_slice(&at.to_be_bytes());
        foot.extend_from_slice(&len.to_be_bytes());
    }
    let hash_at = foot.len();
    foot.extend_from_slice(&[0; 32]);
    foot.extend_from_slice(&(o.at + super::FOOTER_LEN).to_be_bytes());
    foot.extend_from_slice(&VERSION.to_be_bytes());
    foot.extend_from_slice(&MAGIC.to_be_bytes());
    o.sha.update(&foot);
    let hash = std::mem::take(&mut o.sha).finalize();
    foot[hash_at..hash_at + 32].copy_from_slice(&hash);
    o.w.write_all(&foot)?;
    Ok(())
}

/// The name table and the file tree: breadth first, the root at 0 with no
/// name (`0x7FFFFFFF`), each folder's children together and sorted.
fn layout(root: &Dir) -> (Vec<u8>, Vec<u8>) {
    let mut names: Vec<u8> = Vec::new();
    let mut name_at: BTreeMap<String, u32> = BTreeMap::new();
    let mut name = |n: &str| -> u32 {
        if let Some(at) = name_at.get(n) {
            return *at;
        }
        let at = names.len() as u32;
        // Under 128 bytes, as `write` checks.
        names.push(n.len() as u8);
        names.extend_from_slice(n.as_bytes());
        name_at.insert(n.to_owned(), at);
        at
    };
    let children = |d: &'_ Dir| -> Vec<(String, bool)> {
        let mut c: Vec<(String, bool)> = d
            .dirs
            .keys()
            .map(|k| (k.clone(), false))
            .chain(d.files.keys().map(|k| (k.clone(), true)))
            .collect();
        c.sort_by(|a, b| name_order(&a.0, &b.0));
        c
    };
    let mut entries: Vec<[u32; 4]> = vec![[0x7FFF_FFFF, 0, 0, 0]];
    let mut queue: std::collections::VecDeque<(usize, &Dir)> = [(0, root)].into();
    while let Some((at, d)) = queue.pop_front() {
        let kids = children(d);
        entries[at][1] = entries.len() as u32;
        entries[at][2] = kids.len() as u32;
        for (n, file) in kids {
            let index = entries.len();
            if file {
                let (off, size) = d.files[&n];
                entries.push([
                    name(&n) | 0x8000_0000,
                    off as u32,
                    size as u32,
                    ((off >> 32) as u32 & 0xFFFF) | ((size >> 16) as u32 & 0xFFFF_0000),
                ]);
            } else {
                entries.push([name(&n), 0, 0, 0]);
                queue.push_back((index, &d.dirs[&n]));
            }
        }
    }
    let tree = entries
        .iter()
        .flat_map(|e| e.iter().flat_map(|w| w.to_be_bytes()))
        .collect();
    (names, tree)
}
