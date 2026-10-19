use super::*;

// ------------------------------------------------------------ cartridges

/// GBA: fixed byte 0x96 at 0xB2 and the header complement at 0xBD.
pub(super) fn gba(p: &[u8]) -> Option<GameId> {
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
pub(super) fn nds(p: &[u8]) -> Option<GameId> {
    let h = p.get(..0x160)?;
    if u16::from_le_bytes([h[0x15C], h[0x15D]]) != 0xCF56 {
        return None;
    }
    let c = code(&h[0x0C..0x10]);
    let region = c.as_ref().and_then(|c| code_region(c.as_bytes()[3]));
    Some(id("nds", c, text(&h[0..0x0C]), region))
}

/// Game Boy / Color: the header checksum at 0x14D over 0x134..0x14C.
pub(super) fn gb(p: &[u8]) -> Option<GameId> {
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
pub(super) fn n64(p: &[u8]) -> Option<GameId> {
    let h = p.get(..0x40)?;
    if h[..4] != [0x80, 0x37, 0x12, 0x40] {
        return None;
    }
    let c = code(&h[0x3B..0x3F]);
    let region = c.as_ref().and_then(|c| code_region(c.as_bytes()[3]));
    Some(id("n64", c, text(&h[0x20..0x34]), region))
}

/// Mega Drive / Genesis (and 32X): "SEGA" at 0x100.
pub(super) fn md(p: &[u8]) -> Option<GameId> {
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
pub(super) fn snes(p: &[u8]) -> Option<GameId> {
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
