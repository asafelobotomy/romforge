use super::*;

// ------------------------------------------------------------ containers

/// A disc image inside a container romforge can't hash as the DAT's plain
/// image: GameCube / Wii as RVZ, WIA, WBFS, CISO or GCZ; PSP / PS2 as CSO
/// or ZSO; PS1 / PSP as PBP. Gives the game ID (and the plain image's size
/// where the container stores it) by reading just its header and first
/// block, never the whole file.
pub fn read_container<R: std::io::Read + std::io::Seek>(f: &mut R) -> Option<GameId> {
    let mut head = [0u8; 0x100];
    let n = read_at(f, 0, &mut head)?;
    let head = &head[..n];
    let be64 = |b: &[u8]| u64::from_be_bytes(b.try_into().unwrap_or_default());
    let le32 = |b: &[u8]| u32::from_le_bytes(b.try_into().unwrap_or_default());
    let le64 = |b: &[u8]| u64::from_le_bytes(b.try_into().unwrap_or_default());
    match head.get(..4)? {
        // Dolphin's docs/WiaAndRvz.md: big-endian; the ISO's size at 0x24,
        // its first 0x80 bytes at 0x58.
        b"WIA\x01" | b"RVZ\x01" => {
            let mut id = gamecube_wii(head.get(0x58..0xD8)?)?;
            id.image_size = Some(be64(head.get(0x24..0x2C)?));
            Some(id)
        }
        // WBFS: the disc's first 0x100 bytes follow the first HD sector.
        b"WBFS" => {
            let sector = 1u64.checked_shl(*head.get(8)? as u32)?;
            let mut disc = [0u8; 0x100];
            read_at(f, sector, &mut disc)?;
            gamecube_wii(&disc)
        }
        b"CISO" | b"ZISO" => {
            let header = le32(head.get(4..8)?);
            if head[0] == b'C' && header > 0x18 {
                // Wii / GameCube CISO: a block size, a block map, then the
                // disc from 0x8000.
                let mut disc = [0u8; 0x100];
                read_at(f, 0x8000, &mut disc)?;
                return gamecube_wii(&disc);
            }
            // PSP / PS2 CSO (deflate) or ZSO (LZ4).
            let total = le64(head.get(8..16)?);
            let block = le32(head.get(16..20)?) as u64;
            let shift = *head.get(21)? as u32;
            // Header values are untrusted: sane block sizes and shifts only.
            if !(512..=1 << 20).contains(&block) || shift > 31 {
                return None;
            }
            let lz4 = head[0] == b'Z';
            let blocks = (PREFIX_LEN as u64 / block.max(1)).min(total.div_ceil(block.max(1)));
            let mut index = vec![0u8; (blocks as usize + 1) * 4];
            read_at(f, 0x18, &mut index)?;
            let index: Vec<u32> = index.as_chunks::<4>().0.iter().map(|w| le32(w)).collect();
            let mut prefix = Vec::with_capacity(PREFIX_LEN);
            for w in index.windows(2) {
                let plain = w[0] & 0x8000_0000 != 0;
                let start = ((w[0] & 0x7FFF_FFFF) as u64) << shift;
                let end = ((w[1] & 0x7FFF_FFFF) as u64) << shift;
                // A stored block is never much bigger than a plain one.
                let stored = end.checked_sub(start)?;
                if stored > block + 4096 {
                    return None;
                }
                let mut raw = vec![0u8; stored as usize];
                read_at(f, start, &mut raw)?;
                if plain || raw.len() as u64 >= block {
                    raw.truncate(block as usize);
                    prefix.extend_from_slice(&raw);
                } else if lz4 {
                    prefix.extend_from_slice(&lz4_flex::block::decompress(&raw, block as usize).ok()?);
                } else {
                    let mut out = Vec::with_capacity(block as usize);
                    let inflate = flate2::read::DeflateDecoder::new(&raw[..]);
                    std::io::Read::read_to_end(&mut std::io::Read::take(inflate, block), &mut out).ok()?;
                    prefix.extend_from_slice(&out);
                }
            }
            let mut id = disc(&prefix)?;
            id.image_size = Some(total);
            Some(id)
        }
        // PBP (PS1 classics, PSP): PARAM.SFO's offset at 0x08, the next
        // part's at 0x0C.
        b"\0PBP" => {
            let (from, to) = (le32(head.get(8..12)?) as u64, le32(head.get(12..16)?) as u64);
            let mut sfo = vec![0u8; to.checked_sub(from)?.min(64 * 1024) as usize];
            read_at(f, from, &mut sfo)?;
            param_sfo(&sfo)
        }
        _ if head.len() >= 32 && le32(&head[..4]) == 0xB10B_C001 => {
            // Dolphin's GCZ: header, then u64 block pointers and u32 hashes
            // per block, then zlib blocks (top pointer bit: stored plain).
            let size = le64(&head[16..24]);
            let block = (le32(&head[24..28]) as u64).min(1 << 22);
            let blocks = le32(&head[28..32]) as u64;
            let data = 32 + 12 * blocks;
            let mut ptr = [0u8; 16];
            read_at(f, 32, &mut ptr)?;
            let (p0, p1) = (le64(&ptr[..8]), le64(&ptr[8..]));
            let flag = 1u64 << 63;
            let len = if blocks > 1 {
                (p1 & !flag).checked_sub(p0 & !flag)?
            } else {
                0x8000
            };
            let mut raw = vec![0u8; len.min(1 << 22) as usize];
            read_at(f, data.checked_add(p0 & !flag)?, &mut raw)?;
            let first = if p0 & flag != 0 {
                raw
            } else {
                // At most one block's worth, whatever the stream claims.
                let mut out = Vec::new();
                let inflate = flate2::read::ZlibDecoder::new(&raw[..]);
                std::io::Read::read_to_end(&mut std::io::Read::take(inflate, block.max(0x8000)), &mut out)
                    .ok()?;
                out
            };
            let mut id = gamecube_wii(&first)?;
            id.image_size = Some(size);
            Some(id)
        }
        _ => None,
    }
}

/// Read up to `buf.len()` bytes at `at`; how many were read.
fn read_at<R: std::io::Read + std::io::Seek>(f: &mut R, at: u64, buf: &mut [u8]) -> Option<usize> {
    f.seek(std::io::SeekFrom::Start(at)).ok()?;
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..]).ok()? {
            0 => break,
            k => n += k,
        }
    }
    (n > 0).then_some(n)
}

/// PARAM.SFO: DISC_ID and TITLE; CATEGORY "ME" is a PS1 classic.
fn param_sfo(sfo: &[u8]) -> Option<GameId> {
    if sfo.get(..4)? != b"\0PSF" {
        return None;
    }
    let le16 = |b: &[u8]| u16::from_le_bytes([b[0], b[1]]) as usize;
    let le32 = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
    let (keys, data, n) = (
        le32(sfo.get(8..12)?),
        le32(sfo.get(12..16)?),
        le32(sfo.get(16..20)?),
    );
    let field = |want: &str| -> Option<String> {
        for i in 0..n.min(64) {
            let e = sfo.get(20 + i * 16..36 + i * 16)?;
            let key = sfo.get(keys + le16(&e[..2])..)?;
            let key = &key[..key.iter().position(|&b| b == 0)?];
            if key == want.as_bytes() {
                let at = data + le32(&e[12..16]);
                return text(sfo.get(at..at + le32(&e[4..8]))?);
            }
        }
        None
    };
    let kind = if field("CATEGORY").as_deref() == Some("ME") {
        "ps1"
    } else {
        "psp"
    };
    Some(id(kind, field("DISC_ID"), field("TITLE"), None))
}
