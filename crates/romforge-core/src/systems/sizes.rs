use super::*;

/// Games' boxes as they stand on a shelf, in millimetres: the front's
/// width and height, and the spine's thickness, for (North America,
/// Europe, Japan). The fullscreen mode's bookshelf draws them to scale.
/// The one table of box shapes too: [`box_aspect`] reads its fronts.
/// Systems not listed follow [`box_size`]'s rule. From box protector
/// makers' sizes less their margin (RetroProtection, gameboxprotector.com),
/// Japanese package-size listings and collectors' measurements, 2026-10;
/// Neo Geo Pocket, the PC Engine's and SuperGrafx's, the TurboGrafx-16's
/// tall plastic cases, Virtual Boy, Wii U and regions a platform wasn't
/// sold in are estimates.
pub(super) const BOX_SIZES: &[(&str, [[u16; 3]; 3])] = &[
    // Famicom boxes lie down.
    ("NES", [[127, 179, 24], [127, 179, 23], [139, 98, 22]]),
    // The Disk System's soft case.
    ("FDS", [[91, 99, 17], [91, 99, 17], [91, 99, 17]]),
    // The Super Famicom's long and flat.
    ("SNES", [[181, 127, 31], [179, 126, 31], [192, 106, 30]]),
    // Japan's stand up.
    ("N64", [[181, 127, 31], [179, 126, 31], [138, 190, 30]]),
    ("Game Boy", [[115, 127, 23], [115, 127, 23], [97, 124, 22]]),
    ("Game Boy Color", [[115, 127, 23], [115, 127, 23], [97, 124, 22]]),
    (
        "Game Boy Advance",
        [[115, 127, 22], [115, 127, 22], [87, 137, 21]],
    ),
    ("Virtual Boy", [[125, 127, 23], [125, 127, 23], [125, 127, 23]]),
    ("Nintendo DS", [[125, 137, 15], [123, 135, 15], [125, 137, 17]]),
    ("Nintendo 3DS", [[125, 137, 12], [125, 137, 12], [125, 137, 14]]),
    (
        "Nintendo Switch",
        [[105, 170, 11], [105, 170, 11], [105, 170, 11]],
    ),
    // Japan's cases are smaller.
    ("GameCube", [[135, 190, 14], [135, 190, 14], [106, 149, 15]]),
    ("Wii", [[135, 190, 14], [135, 190, 14], [136, 191, 14]]),
    ("Wii U", [[135, 186, 14], [135, 186, 14], [135, 186, 14]]),
    ("Genesis", [[128, 179, 27], [128, 177, 24], [128, 179, 26]]),
    ("Master System", [[128, 179, 26], [128, 177, 24], [139, 98, 22]]),
    ("Sega 32X", [[128, 179, 27], [128, 177, 24], [128, 179, 26]]),
    ("Game Gear", [[112, 160, 27], [111, 157, 25], [112, 160, 25]]),
    // Long boxes; Japan's jewel cases.
    ("Sega CD", [[144, 207, 21], [144, 207, 21], [142, 125, 10]]),
    ("Saturn", [[144, 207, 21], [131, 204, 15], [142, 125, 10]]),
    // Europe's cases twice as thick.
    ("Dreamcast", [[142, 125, 10], [141, 124, 20], [142, 125, 10]]),
    ("Neo Geo", [[171, 219, 42], [171, 219, 42], [171, 219, 42]]),
    ("Neo Geo Pocket", [[100, 130, 20], [100, 130, 20], [100, 130, 20]]),
    // The TurboGrafx-16's tall plastic cases; the PC Engine's jewel-style.
    ("TurboGrafx-16", [[135, 185, 15], [135, 185, 15], [142, 125, 10]]),
    ("SuperGrafx", [[142, 125, 10], [142, 125, 10], [142, 125, 10]]),
    ("Lynx", [[112, 138, 23], [112, 138, 23], [112, 138, 23]]),
    ("Atari 2600", [[140, 194, 23], [140, 194, 23], [140, 194, 23]]),
    ("Atari 7800", [[140, 194, 23], [140, 194, 23], [140, 194, 23]]),
    // Jewel cases, hinge included.
    ("PlayStation", [[142, 125, 10], [142, 125, 10], [142, 130, 15]]),
    ("PlayStation 2", [[135, 190, 14], [135, 190, 14], [136, 190, 16]]),
    ("PlayStation 3", [[135, 171, 14], [135, 171, 14], [135, 171, 15]]),
    ("PlayStation 4", [[135, 170, 14], [135, 170, 14], [134, 170, 14]]),
    ("PSP", [[104, 177, 14], [104, 176, 14], [104, 177, 14]]),
    (
        "PlayStation Vita",
        [[104, 136, 12], [104, 136, 12], [104, 136, 12]],
    ),
    ("Xbox", [[135, 190, 14], [135, 190, 14], [135, 190, 14]]),
    ("Xbox 360", [[135, 190, 14], [135, 190, 14], [135, 190, 15]]),
    ("Xbox One", [[135, 172, 12], [135, 172, 12], [135, 172, 12]]),
    // The US long box.
    ("3DO", [[136, 262, 27], [142, 125, 10], [142, 125, 10]]),
];

/// `system`'s listed box size in `region`, if [`BOX_SIZES`] has it.
pub(super) fn listed_size(system: &System, region: BoxRegion) -> Option<[u16; 3]> {
    let (_, s) = BOX_SIZES.iter().find(|(k, _)| *k == system.key)?;
    Some(
        s[match region {
            BoxRegion::NorthAmerica => 0,
            BoxRegion::Europe => 1,
            BoxRegion::Japan => 2,
        }],
    )
}

/// A box's size for `system` sold in `region`, [width, height, depth] in
/// millimetres: the listed one, else worked out from its front's shape
/// ([`box_aspect`]) and the kind of box it comes in (a computer's big box,
/// a jewel case before the sixth generation, a DVD case after, a
/// handheld's small box, a cartridge's cardboard box).
pub fn box_size(system: &System, region: BoxRegion) -> [u16; 3] {
    if let Some(s) = listed_size(system, region) {
        return s;
    }
    let aspect = box_aspect(system, region);
    // The longer side and the depth of each kind of box.
    let (long, depth) = if is_computer(system) {
        (240.0, 50.0)
    } else if system.is_disc_based {
        if system.generation <= 5 {
            (142.0, 10.0)
        } else {
            (190.0, 14.0)
        }
    } else if system.is_handheld {
        (125.0, 25.0)
    } else {
        (185.0, 35.0)
    };
    let (w, h) = if aspect >= 1.0 {
        (long, long / aspect)
    } else {
        (long * aspect, long)
    };
    [w.round() as u16, h.round() as u16, depth as u16]
}
