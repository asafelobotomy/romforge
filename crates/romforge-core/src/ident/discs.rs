use super::*;

// ------------------------------------------------------------ discs

/// A disc image's 2048-byte user-data sectors, whether stored as raw
/// 2352-byte sectors (BIN tracks, CHD tracks) or as plain 2048 (ISO).
struct Sectors<'a> {
    data: &'a [u8],
    raw: bool,
}

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
pub(super) fn boot_name(name: &str) -> bool {
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
pub(super) fn disc(p: &[u8]) -> Option<GameId> {
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
pub(super) fn gamecube_wii(p: &[u8]) -> Option<GameId> {
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
