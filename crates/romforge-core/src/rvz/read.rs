//! Reading an RVZ: its tables, then (for a GameCube disc) the ISO it holds,
//! group by group, as a stream. A group that can't be read, or decodes to
//! the wrong size, is an error, never a short image.

use std::io::{self, Read, Seek, SeekFrom};

use super::{DISC_LEN, Disc, GAMECUBE, Group, HEAD_LEN, MAGIC, Raw, ZSTD, be32, be64};

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("RVZ: {what}"))
}

/// An open RVZ.
pub struct Rvz<R> {
    file: R,
    pub iso_size: u64,
    pub disc_type: u32,
    disc: Disc,
    raws: Vec<Raw>,
    groups: Vec<Group>,
}

fn read_at<R: Read + Seek>(f: &mut R, at: u64, len: usize) -> io::Result<Vec<u8>> {
    f.seek(SeekFrom::Start(at))?;
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf)?;
    Ok(buf)
}

/// zstd data decoded to at most `limit` bytes.
fn unzstd(data: &[u8], limit: u64) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    crate::z3ds::Frames::new(data).take(limit).read_to_end(&mut out)?;
    Ok(out)
}

impl<R: Read + Seek> Rvz<R> {
    /// `file` as an RVZ, `None` when it isn't one.
    pub fn open(mut file: R) -> io::Result<Option<Rvz<R>>> {
        file.seek(SeekFrom::Start(0))?;
        let mut head = [0u8; HEAD_LEN + DISC_LEN];
        if crate::chunks::read_full(&mut file, &mut head)? < head.len() || head[..4] != MAGIC {
            return Ok(None);
        }
        let iso_size = be64(&head, 0x24);
        let disc = Disc::parse(&head[HEAD_LEN..]).ok_or_else(|| bad("a short disc header"))?;
        if disc.compression != ZSTD && disc.compression != 0 {
            return Err(bad("a compression other than zstd"));
        }
        if disc.chunk < 0x8000 || disc.chunk > 1 << 30 {
            return Err(bad("a chunk size out of range"));
        }
        // Tables: untrusted counts, so bounded by the sizes they decode to.
        let table = |f: &mut R, off: u64, size: u32, count: u32, len: usize| -> io::Result<Vec<u8>> {
            let want = count as u64 * len as u64;
            if want > 1 << 28 {
                return Err(bad("a table too large"));
            }
            let raw = read_at(f, off, size as usize)?;
            let plain = if disc.compression == ZSTD {
                unzstd(&raw, want)?
            } else {
                raw
            };
            if (plain.len() as u64) < want {
                return Err(bad("a table cut short"));
            }
            Ok(plain)
        };
        let raw = table(&mut file, disc.raw_off, disc.raw_size, disc.n_raw, super::RAW_LEN)?;
        let raws = raw
            .as_chunks::<{ super::RAW_LEN }>()
            .0
            .iter()
            .take(disc.n_raw as usize)
            .map(|r| Raw {
                off: be64(r, 0),
                size: be64(r, 8),
                first_group: be32(r, 16),
                groups: be32(r, 20),
            })
            .collect();
        let g = table(
            &mut file,
            disc.group_off,
            disc.group_size,
            disc.n_groups,
            super::GROUP_LEN,
        )?;
        let groups = g
            .as_chunks::<{ super::GROUP_LEN }>()
            .0
            .iter()
            .take(disc.n_groups as usize)
            .map(|r| Group {
                off4: be32(r, 0),
                size: be32(r, 4),
                packed: be32(r, 8),
            })
            .collect();
        Ok(Some(Rvz {
            file,
            iso_size,
            disc_type: disc.disc_type,
            disc,
            raws,
            groups,
        }))
    }

    /// Group `index`'s bytes on the disc: `len` of them from `disc_off`.
    fn group(&mut self, index: u32, disc_off: u64, len: u64) -> io::Result<Vec<u8>> {
        let g = *self
            .groups
            .get(index as usize)
            .ok_or_else(|| bad("a group out of range"))?;
        let size = g.size & !Group::COMPRESSED;
        if size == 0 {
            return Ok(vec![0u8; len as usize]);
        }
        let stored = read_at(&mut self.file, u64::from(g.off4) * 4, size as usize)?;
        let plain = if g.size & Group::COMPRESSED != 0 {
            let limit = if g.packed != 0 { u64::from(g.packed) } else { len };
            unzstd(&stored, limit)?
        } else {
            stored
        };
        let mut out = Vec::with_capacity(len as usize);
        if g.packed != 0 {
            let packed = plain
                .get(..g.packed as usize)
                .ok_or_else(|| bad("packed data cut short"))?;
            super::unpack(packed, disc_off, &mut out).ok_or_else(|| bad("bad packed data"))?;
        } else {
            out = plain;
        }
        if (out.len() as u64) < len {
            return Err(bad("a group decodes short"));
        }
        out.truncate(len as usize);
        Ok(out)
    }

    /// A GameCube disc's ISO, as a stream of exactly `iso_size` bytes.
    pub fn iso(self) -> io::Result<Iso<R>> {
        if self.disc_type != GAMECUBE {
            return Err(bad("only GameCube discs are read as an image"));
        }
        // Each raw-data range starts at a multiple of 0x8000 (the first one
        // says 0x80, holding the disc's start too); its groups follow on.
        let mut plan = Vec::new();
        let mut raws = self.raws.clone();
        raws.sort_by_key(|r| r.off);
        for r in &raws {
            let start = r.off / 0x8000 * 0x8000;
            let end = r
                .off
                .checked_add(r.size)
                .ok_or_else(|| bad("a range out of range"))?;
            for i in 0..r.groups {
                let from = start + u64::from(i) * self.disc.chunk;
                if from >= end {
                    break;
                }
                plan.push((r.first_group + i, from, (end - from).min(self.disc.chunk)));
            }
        }
        Ok(Iso {
            rvz: self,
            plan: plan.into_iter(),
            pos: 0,
            buf: Vec::new(),
            at: 0,
        })
    }
}

/// The ISO an RVZ holds; ranges no group covers read as zeros.
pub struct Iso<R> {
    rvz: Rvz<R>,
    plan: std::vec::IntoIter<(u32, u64, u64)>,
    pos: u64,
    buf: Vec<u8>,
    at: usize,
}

impl<R: Read + Seek> Read for Iso<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let size = self.rvz.iso_size;
        while self.at >= self.buf.len() {
            if self.pos >= size {
                return Ok(0);
            }
            let (data, from) = match self.plan.as_slice().first().copied() {
                Some((g, from, len)) if from <= self.pos => {
                    self.plan.next();
                    (self.rvz.group(g, from, len)?, from)
                }
                next => {
                    let until = next.map_or(size, |(_, f, _)| f).min(size);
                    (vec![0u8; (until - self.pos) as usize], self.pos)
                }
            };
            // The disc's first 0x80 bytes are kept in the header too.
            let mut data = data;
            if from < 0x80 {
                let n = (0x80 - from as usize).min(data.len());
                data[..n].copy_from_slice(&self.rvz.disc.dhead[from as usize..from as usize + n]);
            }
            self.at = (self.pos - from) as usize;
            self.buf = data;
        }
        let n = (self.buf.len() - self.at)
            .min(out.len())
            .min((size - self.pos) as usize);
        out[..n].copy_from_slice(&self.buf[self.at..self.at + n]);
        self.at += n;
        self.pos += n as u64;
        Ok(n)
    }
}
