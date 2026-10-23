use super::*;

/// Where a box was sold, which decides its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxRegion {
    NorthAmerica,
    Europe,
    Japan,
}

impl BoxRegion {
    /// From a game's regions as two-letter codes ([`crate::names::Name`]):
    /// North America for US releases and worldwide ones, Japan only for
    /// Japan-only (or Asia) releases, else Europe.
    pub fn of(codes: &[String]) -> BoxRegion {
        let has = |c: &str| codes.iter().any(|x| x == c);
        if codes.is_empty() || has("US") || has("CA") || codes.len() > 4 {
            BoxRegion::NorthAmerica
        } else if codes
            .iter()
            .all(|c| matches!(c.as_str(), "JP" | "KR" | "TW" | "HK" | "CN" | "AS"))
        {
            BoxRegion::Japan
        } else {
            BoxRegion::Europe
        }
    }
}

/// The shape of games' boxes, front only, width / height in hundredths,
/// for (North America, Europe, Japan): what the picture views draw each
/// box art as. Systems not listed follow [`box_aspect`]'s rule.
const BOX_SHAPES: &[(&str, [u16; 3])] = &[
    // Cartridges: NES portrait, Famicom landscape; SNES and Super Famicom
    // landscape; N64 landscape but Japan's portrait.
    ("NES", [70, 70, 140]),
    ("FDS", [140, 140, 140]),
    ("SNES", [138, 138, 138]),
    ("N64", [138, 138, 72]),
    ("Game Boy", [90, 90, 72]),
    ("Game Boy Color", [90, 90, 72]),
    ("Game Boy Advance", [90, 90, 72]),
    ("Virtual Boy", [90, 90, 72]),
    ("Nintendo DS", [88, 88, 88]),
    ("Nintendo 3DS", [88, 88, 88]),
    ("Nintendo Switch", [62, 62, 62]),
    ("Genesis", [70, 70, 70]),
    ("Master System", [70, 70, 70]),
    ("Sega 32X", [70, 70, 70]),
    ("Game Gear", [72, 72, 72]),
    ("Neo Geo", [70, 70, 70]),
    ("Neo Geo Pocket", [80, 80, 80]),
    ("TurboGrafx-16", [70, 70, 90]),
    ("SuperGrafx", [90, 90, 90]),
    ("Lynx", [75, 75, 75]),
    // Discs: early US and European cases were taller than a jewel case.
    ("Sega CD", [86, 86, 100]),
    ("Saturn", [86, 86, 100]),
    ("Dreamcast", [100, 100, 100]),
    ("PlayStation", [100, 100, 100]),
    ("3DO", [80, 80, 100]),
    ("PSP", [57, 57, 57]),
    ("PlayStation Vita", [82, 82, 82]),
    ("PlayStation 3", [79, 79, 79]),
    ("PlayStation 4", [79, 79, 79]),
    ("Xbox One", [79, 79, 79]),
];

/// A home computer, whose games came in big boxes.
pub(super) fn is_computer(system: &System) -> bool {
    system.folders != Folders::No
        || matches!(
            system.manufacturer,
            "Commodore" | "Sinclair" | "Amstrad" | "Acorn" | "Apple" | "IBM" | "Sharp" | "Fujitsu"
        )
        || matches!(
            system.key,
            "MSX" | "MSX2" | "PC-88" | "PC-98" | "Atari 8-bit" | "Atari ST"
        )
}

/// A box's front, width / height, for `system` sold in `region`: the
/// listed shape, else by rule: discs before the sixth generation in jewel
/// cases (square), later in DVD cases; handhelds' small boxes; home
/// computers' big boxes; arcade flyers and cartridge boxes portrait.
pub fn box_aspect(system: &System, region: BoxRegion) -> f32 {
    if let Some((_, s)) = BOX_SHAPES.iter().find(|(k, _)| *k == system.key) {
        let i = match region {
            BoxRegion::NorthAmerica => 0,
            BoxRegion::Europe => 1,
            BoxRegion::Japan => 2,
        };
        return f32::from(s[i]) / 100.0;
    }
    if is_computer(system) {
        0.8
    } else if system.is_disc_based {
        if system.generation <= 5 { 1.0 } else { 0.71 }
    } else if system.is_handheld {
        0.85
    } else {
        0.72
    }
}
