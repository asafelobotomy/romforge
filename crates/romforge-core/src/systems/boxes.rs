use super::key;
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

/// A home computer, whose games came in big boxes.
pub(super) fn is_computer(system: &System) -> bool {
    system.folders != Folders::No
        || matches!(
            system.manufacturer,
            "Commodore" | "Sinclair" | "Amstrad" | "Acorn" | "Apple" | "IBM" | "Sharp" | "Fujitsu"
        )
        || [
            key!("MSX"),
            key!("MSX2"),
            key!("PC-88"),
            key!("PC-98"),
            key!("Atari 8-bit"),
            key!("Atari ST"),
        ]
        .contains(&system.key)
}

/// A box's front, width / height, for `system` sold in `region`: the
/// listed size's ([`BOX_SIZES`], so the picture views and the bookshelf
/// draw one shape), else by rule: discs before the sixth generation in
/// jewel cases (square), later in DVD cases; handhelds' small boxes; home
/// computers' big boxes; arcade flyers and cartridge boxes portrait.
pub fn box_aspect(system: &System, region: BoxRegion) -> f32 {
    if let Some([w, h, _]) = listed_size(system, region) {
        return f32::from(w) / f32::from(h);
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
