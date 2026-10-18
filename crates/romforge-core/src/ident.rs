//! The one table of game ID readers: what a cartridge's internal header or
//! a disc's boot files say the game is (milestone 12.3).
//!
//! Each reader checks a structure that can't occur by chance (a checksum,
//! a magic, a boot file) before it answers, so no system is guessed from an
//! extension. An ID is evidence only: never a match on its own (house rule
//! 8), since revisions, regions and discs share codes, and a hack keeps its
//! base game's header.

/// Bytes of the data (in the DAT's form: header stripped, byte order fixed)
/// the readers get. A PS1 disc's `SYSTEM.CNF` sits within the first few
/// hundred sectors.
pub const PREFIX_LEN: usize = 1 << 20;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct GameId {
    /// Which reader answered: `snes`, `md`, `gba`, `gb`, `n64`, `nds`,
    /// `ps1`, `ps2`, `psp`, `saturn`, `segacd`, `dc`, `gc`, `wii`.
    pub kind: &'static str,
    /// The product code as the game stores it: a 4-letter game code
    /// (`BPEE`, `NSME`), a Mega Drive serial (`T-95076-00`), a disc's boot
    /// file (`SLUS_007.07`), a GameCube/Wii ID (`GALE01`).
    pub serial: Option<String>,
    /// The internal title, as stored (often upper case, cut short).
    pub title: Option<String>,
    /// Region as the header says it, e.g. `Japan`, `USA`, or a raw code.
    pub region: Option<String>,
    /// For a compressed disc image (RVZ, WIA, GCZ, CSO, ZSO): the size of
    /// the plain image it holds, to compare with the DAT's.
    #[serde(default)]
    pub image_size: Option<u64>,
    /// For an NKit image: the CRC32 its header gives for the original
    /// image (the 4 bytes as stored; their byte order isn't documented).
    #[serde(default)]
    pub source_crc: Option<u32>,
}

impl GameId {
    /// Back from the `files` columns (`id_kind`, ...); `None` without a kind.
    pub fn from_columns(
        kind: Option<String>,
        serial: Option<String>,
        title: Option<String>,
        region: Option<String>,
        image_size: Option<u64>,
        source_crc: Option<u32>,
    ) -> Option<GameId> {
        let kind = KINDS.iter().find(|k| Some(**k) == kind.as_deref())?;
        Some(GameId {
            kind,
            serial,
            title,
            region,
            image_size,
            source_crc,
        })
    }
}

/// Every `kind` a reader gives.
const KINDS: &[&str] = &[
    "snes", "md", "gba", "gb", "n64", "nds", "ps1", "ps2", "psp", "saturn", "segacd", "dc", "gc", "wii",
];

/// Cartridges before discs; the first reader that answers wins.
type Reader = fn(&[u8]) -> Option<GameId>;

const READERS: &[Reader] = &[gba, nds, gb, n64, md, snes, disc, gamecube_wii];

/// What the first bytes of a file say it is, if any reader recognises them.
pub fn read(prefix: &[u8]) -> Option<GameId> {
    let id = READERS.iter().find_map(|r| r(prefix))?;
    debug_assert!(KINDS.contains(&id.kind), "{} not in KINDS", id.kind);
    Some(id)
}

/// Printable text from a fixed-size field: stops at NUL, trims spaces.
fn text(bytes: &[u8]) -> Option<String> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let s: String = bytes[..end]
        .iter()
        .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { ' ' })
        .collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    (!s.is_empty()).then_some(s)
}

/// A game code: letters and digits only, exactly as long as the field.
fn code(bytes: &[u8]) -> Option<String> {
    bytes
        .iter()
        .all(u8::is_ascii_alphanumeric)
        .then(|| String::from_utf8_lossy(bytes).into_owned())
}

fn id(kind: &'static str, serial: Option<String>, title: Option<String>, region: Option<&str>) -> GameId {
    GameId {
        kind,
        serial,
        title,
        region: region.map(str::to_owned),
        image_size: None,
        source_crc: None,
    }
}

/// The region a Nintendo game code's last letter stands for.
fn code_region(c: u8) -> Option<&'static str> {
    Some(match c {
        b'J' => "Japan",
        b'E' => "USA",
        b'P' => "Europe",
        b'D' => "Germany",
        b'F' => "France",
        b'I' => "Italy",
        b'S' => "Spain",
        b'U' => "Australia",
        b'K' => "Korea",
        b'C' => "China",
        b'A' => "World",
        _ => return None,
    })
}

// ------------------------------------------------------------ cartridges

/// GBA: fixed byte 0x96 at 0xB2 and the header complement at 0xBD.
fn gba(p: &[u8]) -> Option<GameId> {
    let h = p.get(..0xC0)?;
    let sum = h[0xA0..0xBD].iter().fold(0u8, |a, &b| a.wrapping_sub(b));
    if h[0xB2] != 0x96 || sum.wrapping_sub(0x19) != h[0xBD] {
        return None;
    }
    let c = code(&h[0xAC..0xB0]);
    let region = c.as_ref().and_then(|c| code_region(c.as_bytes()[3]));
    Some(id("gba", c, text(&h[0xA0..0xAC]), region))
}

/// Nintendo DS: the CRC16 of the logo is always CF56.
fn nds(p: &[u8]) -> Option<GameId> {
    let h = p.get(..0x160)?;
    if u16::from_le_bytes([h[0x15C], h[0x15D]]) != 0xCF56 {
        return None;
    }
    let c = code(&h[0x0C..0x10]);
    let region = c.as_ref().and_then(|c| code_region(c.as_bytes()[3]));
    Some(id("nds", c, text(&h[0..0x0C]), region))
}

/// Game Boy / Color: the header checksum at 0x14D over 0x134..0x14C.
fn gb(p: &[u8]) -> Option<GameId> {
    let h = p.get(..0x150)?;
    let sum = h[0x134..0x14D]
        .iter()
        .fold(0u8, |a, &b| a.wrapping_sub(b).wrapping_sub(1));
    // A blank header checksums too; require some title.
    if sum != h[0x14D] || h[0x134] == 0 {
        return None;
    }
    // Colour-era carts keep a 4-letter code at 0x13F and a flag at 0x143.
    let cgb = h[0x143] & 0x80 != 0;
    let (title, c) = if cgb {
        let c = code(&h[0x13F..0x143]).filter(|c| c.len() == 4);
        let end = if c.is_some() { 0x13F } else { 0x143 };
        (text(&h[0x134..end]), c)
    } else {
        (text(&h[0x134..0x144]), None)
    };
    let region = if h[0x14A] == 0 { "Japan" } else { "Not Japan" };
    Some(id("gb", c, title, Some(region)))
}

/// N64 in the DAT's (`.z64`) byte order.
fn n64(p: &[u8]) -> Option<GameId> {
    let h = p.get(..0x40)?;
    if h[..4] != [0x80, 0x37, 0x12, 0x40] {
        return None;
    }
    let c = code(&h[0x3B..0x3F]);
    let region = c.as_ref().and_then(|c| code_region(c.as_bytes()[3]));
    Some(id("n64", c, text(&h[0x20..0x34]), region))
}

/// Mega Drive / Genesis (and 32X): "SEGA" at 0x100.
fn md(p: &[u8]) -> Option<GameId> {
    let h = p.get(..0x200)?;
    if !h[0x100..0x110].windows(4).take(2).any(|w| w == b"SEGA") {
        return None;
    }
    // "GM T-95076-00": the type ("GM" game, "AI" education, ...) goes.
    let serial = text(&h[0x180..0x18E]).map(|s| match s.split_once(' ') {
        Some((kind, rest)) if kind.len() == 2 => rest.trim().to_owned(),
        _ => s,
    });
    let title = text(&h[0x150..0x180]).or_else(|| text(&h[0x120..0x150]));
    let region = text(&h[0x1F0..0x1F3]);
    Some(GameId {
        kind: "md",
        serial,
        title,
        region,
        image_size: None,
        source_crc: None,
    })
}

/// SNES: the checksum and its complement at LoROM 0x7FC0 or HiROM 0xFFC0
/// (copier header already stripped).
fn snes(p: &[u8]) -> Option<GameId> {
    for base in [0x7FC0usize, 0xFFC0] {
        let Some(h) = p.get(base..base + 0x30) else {
            continue;
        };
        let complement = u16::from_le_bytes([h[0x1C], h[0x1D]]);
        let checksum = u16::from_le_bytes([h[0x1E], h[0x1F]]);
        if complement ^ checksum != 0xFFFF || text(&h[..21]).is_none() {
            continue;
        }
        // A 4-letter game code in the extended header (maker code 0x33).
        let c = (h[0x1A] == 0x33)
            .then(|| p.get(base - 0x0E..base - 0x0A).and_then(code))
            .flatten();
        let region = match h[0x19] {
            0 => Some("Japan"),
            1 => Some("USA"),
            2 => Some("Europe"),
            _ => None,
        };
        return Some(id("snes", c, text(&h[..21]), region));
    }
    None
}

// ------------------------------------------------------------ discs

/// A disc image's 2048-byte user-data sectors, whether stored as raw
/// 2352-byte sectors (BIN tracks, CHD tracks) or as plain 2048 (ISO).
struct Sectors<'a> {
    data: &'a [u8],
    raw: bool,
}

const SYNC: [u8; 12] = [0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0];

impl<'a> Sectors<'a> {
    fn new(data: &'a [u8]) -> Self {
        Sectors {
            data,
            raw: data.starts_with(&SYNC),
        }
    }

    fn get(&self, lba: usize) -> Option<&'a [u8]> {
        if self.raw {
            let s = self.data.get(lba * 2352..(lba + 1) * 2352)?;
            // Mode 1 data starts at 16; Mode 2 (form 1) after the subheader.
            let at = if s[15] == 2 { 24 } else { 16 };
            s.get(at..at + 2048)
        } else {
            self.data.get(lba * 2048..(lba + 1) * 2048)
        }
    }

    /// The ISO 9660 root directory: (name without `;1`, first sector,
    /// bytes) per entry.
    fn root(&self) -> Option<Vec<(String, usize, usize)>> {
        let pvd = self.get(16)?;
        if pvd[0] != 1 || &pvd[1..6] != b"CD001" {
            return None;
        }
        let root = &pvd[156..190];
        let extent = u32::from_le_bytes(root[2..6].try_into().ok()?) as usize;
        let size = u32::from_le_bytes(root[10..14].try_into().ok()?) as usize;
        let mut out = Vec::new();
        for n in 0..size.div_ceil(2048).min(16) {
            let dir = self.get(extent + n)?;
            let mut at = 0;
            while at + 33 < dir.len() && dir[at] != 0 {
                let rec = &dir[at..(at + dir[at] as usize).min(dir.len())];
                at += dir[at] as usize;
                let len = *rec.get(32)? as usize;
                let name = rec.get(33..33 + len)?;
                let name = name.split(|&b| b == b';').next().unwrap_or(name);
                let lba = u32::from_le_bytes(rec[2..6].try_into().ok()?) as usize;
                let bytes = u32::from_le_bytes(rec[10..14].try_into().ok()?) as usize;
                out.push((String::from_utf8_lossy(name).into_owned(), lba, bytes));
            }
        }
        Some(out)
    }

    /// A file in the root directory, by name, if its data is at hand.
    fn root_file(&self, name: &str) -> Option<Vec<u8>> {
        let (_, lba, bytes) = self
            .root()?
            .into_iter()
            .find(|(n, ..)| n.eq_ignore_ascii_case(name))?;
        let mut out = Vec::new();
        for s in 0..bytes.div_ceil(2048).min(8) {
            out.extend_from_slice(self.get(lba + s)?);
        }
        out.truncate(bytes);
        Some(out)
    }
}

/// A PlayStation boot executable's name: `SLUS_007.47` (four letters, an
/// underscore, three digits, a dot, two digits).
fn boot_name(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() == 11
        && b[..4].iter().all(u8::is_ascii_uppercase)
        && b[4] == b'_'
        && b[5..8].iter().all(u8::is_ascii_digit)
        && b[8] == b'.'
        && b[9..].iter().all(u8::is_ascii_digit)
}

/// PS1/PS2 (`SYSTEM.CNF`), PSP (`UMD_DATA.BIN`), or a Sega disc (IP.BIN in
/// sector 0).
fn disc(p: &[u8]) -> Option<GameId> {
    let s = Sectors::new(p);
    if let Some(first) = s.get(0)
        && let Some(id) = sega(first)
    {
        return Some(id);
    }
    if let Some(cnf) = s.root_file("SYSTEM.CNF") {
        let cnf = String::from_utf8_lossy(&cnf);
        for line in cnf.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let kind = match key.trim().to_ascii_uppercase().as_str() {
                "BOOT" => "ps1",
                "BOOT2" => "ps2",
                _ => continue,
            };
            // cdrom:\SLUS_007.07;1 or cdrom0:\SLUS_203.12;1
            let file = value.trim().rsplit(['\\', ':']).next().unwrap_or("");
            let file = file.split(';').next().unwrap_or("").trim();
            return Some(id(kind, (!file.is_empty()).then(|| file.to_owned()), None, None));
        }
    }
    if let Some(umd) = s.root_file("UMD_DATA.BIN") {
        let serial = umd.split(|&b| b == b'|').next().and_then(text);
        return Some(id("psp", serial, None, None));
    }
    // SYSTEM.CNF may lie far into the disc (Resident Evil's is ~690 MB in);
    // the root then still names the boot file. CD images are taken as PS1,
    // DVD images (plain 2048-byte sectors) as PS2.
    let root = s.root()?;
    let has_cnf = root.iter().any(|(n, ..)| n.eq_ignore_ascii_case("SYSTEM.CNF"));
    let boot = root.into_iter().map(|(n, ..)| n).find(|n| boot_name(n))?;
    has_cnf.then(|| id(if s.raw { "ps1" } else { "ps2" }, Some(boot), None, None))
}

/// Sega Saturn, Sega CD / Mega-CD and Dreamcast boot headers.
fn sega(s: &[u8]) -> Option<GameId> {
    if s.starts_with(b"SEGA SEGASATURN") {
        return Some(id(
            "saturn",
            text(&s[0x20..0x2A]),
            text(&s[0x60..0xD0]),
            text(&s[0x40..0x50]).as_deref(),
        ));
    }
    if s.starts_with(b"SEGA SEGAKATANA") {
        return Some(id(
            "dc",
            text(&s[0x40..0x4A]),
            text(&s[0x80..0x100]),
            text(&s[0x30..0x38]).as_deref(),
        ));
    }
    if s.starts_with(b"SEGADISCSYSTEM") {
        let mut m = md(s)?;
        m.kind = "segacd";
        return Some(m);
    }
    None
}

/// GameCube and Wii discs (plain images): their magic words.
fn gamecube_wii(p: &[u8]) -> Option<GameId> {
    let h = p.get(..0x60)?;
    let kind = if h[0x1C..0x20] == [0xC2, 0x33, 0x9F, 0x3D] {
        "gc"
    } else if h[0x18..0x1C] == [0x5D, 0x1C, 0x9E, 0xA3] {
        "wii"
    } else {
        return None;
    };
    let c = code(&h[..6]);
    let region = c.as_ref().and_then(|c| code_region(c.as_bytes()[3]));
    let mut out = id(kind, c, text(&h[0x20..0x60]), region);
    // NKit (junk data removed): "NKIT" at 0x200, the original image's
    // CRC32 at 0x208.
    if p.get(0x200..0x204) == Some(b"NKIT") {
        out.source_crc = p
            .get(0x208..0x20C)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    }
    Some(out)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gba_header() {
        let mut rom = vec![0u8; 0xC0];
        rom[0xA0..0xAC].copy_from_slice(b"POKEMON EMER");
        rom[0xAC..0xB0].copy_from_slice(b"BPEE");
        rom[0xB0..0xB2].copy_from_slice(b"01");
        rom[0xB2] = 0x96;
        let sum = rom[0xA0..0xBD].iter().fold(0u8, |a, &b| a.wrapping_sub(b));
        rom[0xBD] = sum.wrapping_sub(0x19);
        let id = read(&rom).unwrap();
        assert_eq!(id.kind, "gba");
        assert_eq!(id.serial.as_deref(), Some("BPEE"));
        assert_eq!(id.title.as_deref(), Some("POKEMON EMER"));
        assert_eq!(id.region.as_deref(), Some("USA"));
        rom[0xBD] ^= 1;
        assert_eq!(read(&rom), None, "a bad checksum is no header");
    }

    #[test]
    fn snes_lorom_with_game_code() {
        let mut rom = vec![0xFFu8; 0x8000];
        let h = 0x7FC0;
        rom[h..h + 21].copy_from_slice(b"DRAGONQUEST3         ");
        rom[h + 0x19] = 0;
        rom[h + 0x1A] = 0x33;
        rom[h - 0x0E..h - 0x0A].copy_from_slice(b"AQ3J");
        rom[h + 0x1C..h + 0x20].copy_from_slice(&[0x1A, 0x93, 0xE5, 0x6C]);
        let id = read(&rom).unwrap();
        assert_eq!(id.kind, "snes");
        assert_eq!(id.title.as_deref(), Some("DRAGONQUEST3"));
        assert_eq!(id.serial.as_deref(), Some("AQ3J"));
        assert_eq!(id.region.as_deref(), Some("Japan"));
    }

    #[test]
    fn mega_drive_serial_without_its_type() {
        let mut rom = vec![0u8; 0x200];
        rom[0x100..0x110].copy_from_slice(b"SEGA GENESIS    ");
        rom[0x150..0x166].copy_from_slice(b"CASTLEVANIA BLOODLINES");
        rom[0x180..0x18E].copy_from_slice(b"GM T-95076-00 ");
        rom[0x1F0] = b'U';
        let id = read(&rom).unwrap();
        assert_eq!(id.kind, "md");
        assert_eq!(id.serial.as_deref(), Some("T-95076-00"));
        assert_eq!(id.title.as_deref(), Some("CASTLEVANIA BLOODLINES"));
        assert_eq!(id.region.as_deref(), Some("U"));
    }

    #[test]
    fn n64_and_game_boy() {
        let mut z64 = vec![0u8; 0x40];
        z64[..4].copy_from_slice(&[0x80, 0x37, 0x12, 0x40]);
        z64[0x20..0x2E].copy_from_slice(b"SUPER MARIO 64");
        z64[0x3B..0x3F].copy_from_slice(b"NSME");
        let id = read(&z64).unwrap();
        assert_eq!((id.kind, id.serial.as_deref()), ("n64", Some("NSME")));
        assert_eq!(id.region.as_deref(), Some("USA"));

        let mut gb = vec![0u8; 0x150];
        gb[0x134..0x13A].copy_from_slice(b"TETRIS");
        gb[0x14A] = 1;
        gb[0x14D] = gb[0x134..0x14D]
            .iter()
            .fold(0u8, |a, &b| a.wrapping_sub(b).wrapping_sub(1));
        let id = read(&gb).unwrap();
        assert_eq!((id.kind, id.title.as_deref()), ("gb", Some("TETRIS")));
    }

    /// A tiny ISO 9660 image with SYSTEM.CNF in its root, as raw Mode 2
    /// sectors like a PS1 BIN track.
    fn ps1_disc(cnf: &str) -> Vec<u8> {
        let mut user = vec![[0u8; 2048]; 20];
        let pvd = &mut user[16];
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        let root = &mut pvd[156..190];
        root[0] = 34;
        root[2..6].copy_from_slice(&18u32.to_le_bytes());
        root[10..14].copy_from_slice(&2048u32.to_le_bytes());
        let dir = &mut user[18];
        let name = b"SYSTEM.CNF;1";
        dir[0] = (33 + name.len()) as u8;
        dir[2..6].copy_from_slice(&19u32.to_le_bytes());
        dir[10..14].copy_from_slice(&(cnf.len() as u32).to_le_bytes());
        dir[32] = name.len() as u8;
        dir[33..33 + name.len()].copy_from_slice(name);
        user[19][..cnf.len()].copy_from_slice(cnf.as_bytes());
        let mut raw = Vec::new();
        for s in &user {
            let mut sector = [0u8; 2352];
            sector[..12].copy_from_slice(&SYNC);
            sector[15] = 2;
            sector[24..24 + 2048].copy_from_slice(s);
            raw.extend_from_slice(&sector);
        }
        raw
    }

    #[test]
    fn ps1_and_ps2_boot_files() {
        let id = read(&ps1_disc("BOOT = cdrom:\\SLUS_007.07;1\r\nTCB = 4\r\n")).unwrap();
        assert_eq!((id.kind, id.serial.as_deref()), ("ps1", Some("SLUS_007.07")));
        let id = read(&ps1_disc("BOOT2 = cdrom0:\\SLUS_203.12;1\nVER = 1.00\n")).unwrap();
        assert_eq!((id.kind, id.serial.as_deref()), ("ps2", Some("SLUS_203.12")));
        assert_eq!(read(&ps1_disc("nothing here")), None);
    }

    #[test]
    fn boot_file_named_in_the_root_when_system_cnf_is_far() {
        // SYSTEM.CNF's data beyond the prefix: the root's SLUS_007.47 tells.
        let mut disc = ps1_disc("BOOT = cdrom:\\SLUS_007.47;1\r\n");
        let dir = 18 * 2352 + 24;
        let at = dir + disc[dir] as usize;
        let name = b"SLUS_007.47;1";
        disc[at] = (33 + name.len()) as u8;
        disc[at + 32] = name.len() as u8;
        disc[at + 33..at + 33 + name.len()].copy_from_slice(name);
        // Point SYSTEM.CNF far away.
        disc[dir + 2..dir + 6].copy_from_slice(&293_729u32.to_le_bytes());
        let id = read(&disc).unwrap();
        assert_eq!((id.kind, id.serial.as_deref()), ("ps1", Some("SLUS_007.47")));
        assert!(boot_name("SCES_123.45") && !boot_name("ZNULL.DAT") && !boot_name("SLUS-00747"));
    }

    #[test]
    fn saturn_and_gamecube() {
        let mut ip = vec![0u8; 2048];
        ip[..16].copy_from_slice(b"SEGA SEGASATURN ");
        ip[0x20..0x2A].copy_from_slice(b"MK-81009  ");
        ip[0x40..0x41].copy_from_slice(b"U");
        ip[0x60..0x6F].copy_from_slice(b"NIGHTS INTO DRE");
        let id = read(&ip).unwrap();
        assert_eq!((id.kind, id.serial.as_deref()), ("saturn", Some("MK-81009")));

        let mut gc = vec![0u8; 0x60];
        gc[..6].copy_from_slice(b"GALE01");
        gc[0x1C..0x20].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]);
        gc[0x20..0x34].copy_from_slice(b"Super Smash Bros. Me");
        let id = read(&gc).unwrap();
        assert_eq!((id.kind, id.serial.as_deref()), ("gc", Some("GALE01")));
        assert_eq!(id.region.as_deref(), Some("USA"));
    }

    /// A GameCube disc's first bytes.
    fn gc_disc() -> Vec<u8> {
        let mut d = vec![0u8; 0x8000];
        d[..6].copy_from_slice(b"GALE01");
        d[0x1C..0x20].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]);
        d[0x20..0x31].copy_from_slice(b"Super Smash Bros.");
        d
    }

    fn container(bytes: &[u8]) -> Option<GameId> {
        read_container(&mut std::io::Cursor::new(bytes))
    }

    #[test]
    fn gamecube_and_wii_containers() {
        let disc = gc_disc();
        // RVZ / WIA: big-endian ISO size at 0x24, disc head at 0x58.
        let mut rvz = vec![0u8; 0x100];
        rvz[..4].copy_from_slice(b"RVZ\x01");
        rvz[0x24..0x2C].copy_from_slice(&1_459_978_240u64.to_be_bytes());
        rvz[0x58..0xD8].copy_from_slice(&disc[..0x80]);
        let id = container(&rvz).unwrap();
        assert_eq!((id.kind, id.serial.as_deref()), ("gc", Some("GALE01")));
        assert_eq!(id.image_size, Some(1_459_978_240));
        // WBFS: disc head after the first HD sector (2^9 = 512).
        let mut wbfs = vec![0u8; 0x400];
        wbfs[..4].copy_from_slice(b"WBFS");
        wbfs[8] = 9;
        wbfs[0x200..0x300].copy_from_slice(&disc[..0x100]);
        assert_eq!(container(&wbfs).unwrap().serial.as_deref(), Some("GALE01"));
        // Wii/GC CISO: block size, map, disc at 0x8000.
        let mut ciso = vec![0u8; 0x8000];
        ciso[..4].copy_from_slice(b"CISO");
        ciso[4..8].copy_from_slice(&0x20_0000u32.to_le_bytes());
        ciso.extend_from_slice(&disc[..0x100]);
        assert_eq!(container(&ciso).unwrap().serial.as_deref(), Some("GALE01"));
        // GCZ: header, 2 block pointers and hashes, zlib blocks.
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut z, &disc).unwrap();
        let block0 = z.finish().unwrap();
        let mut gcz = Vec::new();
        gcz.extend_from_slice(&0xB10B_C001u32.to_le_bytes());
        gcz.extend_from_slice(&0u32.to_le_bytes());
        gcz.extend_from_slice(&0u64.to_le_bytes());
        gcz.extend_from_slice(&1_459_978_240u64.to_le_bytes());
        gcz.extend_from_slice(&0x8000u32.to_le_bytes());
        gcz.extend_from_slice(&2u32.to_le_bytes());
        gcz.extend_from_slice(&0u64.to_le_bytes());
        gcz.extend_from_slice(&(block0.len() as u64).to_le_bytes());
        gcz.extend_from_slice(&[0u8; 8]);
        gcz.extend_from_slice(&block0);
        let id = container(&gcz).unwrap();
        assert_eq!(
            (id.serial.as_deref(), id.image_size),
            (Some("GALE01"), Some(1_459_978_240))
        );
    }

    /// A plain 2048-byte-sector ISO with one file in its root.
    fn iso(name: &str, content: &[u8]) -> Vec<u8> {
        let mut user = vec![[0u8; 2048]; 24];
        let pvd = &mut user[16];
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        pvd[156] = 34;
        pvd[158..162].copy_from_slice(&18u32.to_le_bytes());
        pvd[166..170].copy_from_slice(&2048u32.to_le_bytes());
        let n = format!("{name};1");
        let dir = &mut user[18];
        dir[0] = (33 + n.len()) as u8;
        dir[2..6].copy_from_slice(&19u32.to_le_bytes());
        dir[10..14].copy_from_slice(&(content.len() as u32).to_le_bytes());
        dir[32] = n.len() as u8;
        dir[33..33 + n.len()].copy_from_slice(n.as_bytes());
        user[19][..content.len()].copy_from_slice(content);
        user.concat()
    }

    /// CSO (deflate) or ZSO (LZ4) of `image`, 2048-byte blocks.
    fn cso(image: &[u8], lz4: bool) -> Vec<u8> {
        let blocks: Vec<Vec<u8>> = image
            .chunks(2048)
            .map(|b| {
                if lz4 {
                    lz4_flex::block::compress(b)
                } else {
                    let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
                    std::io::Write::write_all(&mut e, b).unwrap();
                    e.finish().unwrap()
                }
            })
            .collect();
        let mut out = Vec::new();
        out.extend_from_slice(if lz4 { b"ZISO" } else { b"CISO" });
        out.extend_from_slice(&0x18u32.to_le_bytes());
        out.extend_from_slice(&(image.len() as u64).to_le_bytes());
        out.extend_from_slice(&2048u32.to_le_bytes());
        out.extend_from_slice(&[1, 0, 0, 0]);
        let mut at = 0x18 + 4 * (blocks.len() as u32 + 1);
        let mut index = Vec::new();
        for b in &blocks {
            index.extend_from_slice(&at.to_le_bytes());
            at += b.len() as u32;
        }
        index.extend_from_slice(&at.to_le_bytes());
        out.extend_from_slice(&index);
        for b in &blocks {
            out.extend_from_slice(b);
        }
        out
    }

    #[test]
    fn psp_cso_zso_and_pbp() {
        let image = iso("UMD_DATA.BIN", b"ULUS-10041|0001|G|");
        assert_eq!(read(&image).unwrap().serial.as_deref(), Some("ULUS-10041"));
        for lz4 in [false, true] {
            let id = container(&cso(&image, lz4)).unwrap();
            assert_eq!(
                (id.kind, id.serial.as_deref()),
                ("psp", Some("ULUS-10041")),
                "lz4 {lz4}"
            );
            assert_eq!(id.image_size, Some(image.len() as u64));
        }
        // PBP: PARAM.SFO with DISC_ID, TITLE and CATEGORY ME (a PS1 classic).
        let fields = [
            ("CATEGORY", "ME"),
            ("DISC_ID", "SLUS00067"),
            ("TITLE", "Castlevania SOTN"),
        ];
        let mut keys = Vec::new();
        let mut data = Vec::new();
        let mut entries = Vec::new();
        for (k, v) in fields {
            entries.extend_from_slice(&(keys.len() as u16).to_le_bytes());
            entries.extend_from_slice(&0x0204u16.to_le_bytes());
            entries.extend_from_slice(&(v.len() as u32 + 1).to_le_bytes());
            entries.extend_from_slice(&(v.len() as u32 + 1).to_le_bytes());
            entries.extend_from_slice(&(data.len() as u32).to_le_bytes());
            keys.extend_from_slice(k.as_bytes());
            keys.push(0);
            data.extend_from_slice(v.as_bytes());
            data.push(0);
        }
        let keys_at = 20 + entries.len();
        let mut sfo = b"\0PSF".to_vec();
        sfo.extend_from_slice(&0x0101u32.to_le_bytes());
        sfo.extend_from_slice(&(keys_at as u32).to_le_bytes());
        sfo.extend_from_slice(&((keys_at + keys.len()) as u32).to_le_bytes());
        sfo.extend_from_slice(&(fields.len() as u32).to_le_bytes());
        sfo.extend_from_slice(&entries);
        sfo.extend_from_slice(&keys);
        sfo.extend_from_slice(&data);
        let mut pbp = b"\0PBP".to_vec();
        pbp.extend_from_slice(&0x10000u32.to_le_bytes());
        let after = 0x28 + sfo.len() as u32;
        pbp.extend_from_slice(&0x28u32.to_le_bytes());
        for _ in 0..7 {
            pbp.extend_from_slice(&after.to_le_bytes());
        }
        pbp.extend_from_slice(&sfo);
        let id = container(&pbp).unwrap();
        assert_eq!((id.kind, id.serial.as_deref()), ("ps1", Some("SLUS00067")));
        assert_eq!(id.title.as_deref(), Some("Castlevania SOTN"));
        assert_eq!(container(&[0x5A; 512]), None);
    }

    /// Hostile CSO headers (a shift past 31, an absurd stored block, a
    /// huge block size) give no ID instead of panicking or allocating.
    #[test]
    fn hostile_cso_headers() {
        let good = cso(&iso("UMD_DATA.BIN", b"ULUS-10041|0001|G|"), false);
        assert!(container(&good).is_some());
        let mut shift = good.clone();
        shift[21] = 40;
        assert_eq!(container(&shift), None);
        let mut block = good.clone();
        block[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(container(&block), None);
        let mut stored = good.clone();
        stored[21] = 31;
        stored[0x1C..0x20].copy_from_slice(&0x7FFF_FFFFu32.to_le_bytes());
        assert_eq!(container(&stored), None);
    }

    #[test]
    fn every_kind_has_a_system() {
        for k in KINDS {
            assert!(crate::systems::by_ident(k).next().is_some(), "{k} has no system");
        }
    }

    #[test]
    fn plain_data_has_no_id() {
        assert_eq!(read(&[0x5Au8; 4096]), None);
        assert_eq!(read(&[]), None);
    }
}
