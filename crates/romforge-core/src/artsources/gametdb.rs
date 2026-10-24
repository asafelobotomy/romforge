//! GameTDB (gametdb.com): covers, full covers, 3D boxes and disc or
//! cartridge labels for GameCube, Wii and DS games, by the game ID their
//! header gives (house rule 8: an ID, never a title). Free and keyless;
//! its terms ask software to link to it, which Details and Credits do.
//! It has no API: its files are at fixed addresses
//! (`art.gametdb.com/<platform>/<type>/<region>/<ID>.<ext>`), and a missing
//! one is a 404, so each kind's addresses are offered best first and the
//! first that exists is taken.

use super::{Bases, Found, Job};
use crate::Error;
use crate::enrich::{ArtFound, ArtKind};

/// romforge's header kinds and GameTDB's platform folder for them.
pub const PLATFORMS: &[(&str, &str)] = &[("gc", "wii"), ("wii", "wii"), ("nds", "ds")];

/// Per platform: (kind, GameTDB's types best first, file extension).
fn types(platform: &str) -> &'static [(ArtKind, &'static [&'static str], &'static str)] {
    match platform {
        "ds" => &[
            (ArtKind::Boxart, &["coverHQ", "cover"], "jpg"),
            (ArtKind::Boxfull, &["coverfullHQ", "coverfull"], "jpg"),
            (ArtKind::Box3d, &["cover3D"], "png"),
            (ArtKind::Media, &["cart"], "png"),
        ],
        _ => &[
            (ArtKind::Boxart, &["coverHQ", "cover"], "png"),
            (ArtKind::Boxfull, &["coverfullHQ", "coverfull"], "png"),
            (ArtKind::Box3d, &["cover3D"], "png"),
            (ArtKind::Media, &["disc"], "png"),
        ],
    }
}

/// GameTDB's region folders for a game ID, best first: the one its 4th
/// character (the region letter) names, then English PAL and the US.
pub fn regions(id: &str) -> Vec<&'static str> {
    let own = match id.chars().nth(3) {
        Some('E') => "US",
        Some('J') => "JA",
        Some('K') => "KO",
        Some('D') => "DE",
        Some('F') => "FR",
        Some('S') => "ES",
        Some('I') => "IT",
        Some('H') => "NL",
        Some('U') => "AU",
        Some('W') => "ZH",
        _ => "EN",
    };
    let mut out = vec![own];
    for r in ["EN", "US"] {
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

pub(super) fn lookup(bases: &Bases, job: &Job) -> Result<Found, Error> {
    let Some(id) = job.serial.as_ref() else {
        return Ok(Found::default());
    };
    let (Some(serial), Some((_, platform))) = (
        id.serial.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        PLATFORMS.iter().find(|(k, _)| *k == id.kind),
    ) else {
        return Ok(Found::default());
    };
    let mut art = Vec::new();
    for (kind, names, ext) in types(platform) {
        for name in *names {
            for region in regions(serial) {
                art.push(ArtFound {
                    kind: *kind,
                    url: format!("{}/{platform}/{name}/{region}/{serial}.{ext}", bases.gametdb),
                    thumb: None,
                    label: Some(format!("GameTDB {region}")),
                });
            }
        }
    }
    Ok(Found {
        art,
        fields: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_follow_the_platform_and_the_id_s_region() {
        assert_eq!(regions("RMGE01"), ["US", "EN"]);
        assert_eq!(regions("RMGP01"), ["EN", "US"]);
        assert_eq!(regions("AMCJ"), ["JA", "EN", "US"]);
        let system = crate::systems::by_key("Nintendo DS").unwrap();
        let serial = crate::ident::GameId {
            kind: "nds",
            serial: Some("AMCE".into()),
            ..Default::default()
        };
        let job = Job {
            system,
            title: "x",
            rom: None,
            prefs: &[],
            language: "en",
            year: None,
            serial: Some(serial),
            video: false,
        };
        let f = lookup(&Bases::default(), &job).unwrap();
        assert_eq!(f.art[0].url, "https://art.gametdb.com/ds/coverHQ/US/AMCE.jpg");
        assert!(
            f.art
                .iter()
                .any(|a| a.kind == ArtKind::Media && a.url.ends_with("/ds/cart/US/AMCE.png"))
        );
    }
}
