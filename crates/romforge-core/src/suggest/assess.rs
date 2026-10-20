//! Weighing one candidate on everything known.

use std::collections::BTreeMap;

use rusqlite::{Connection, params};

use super::{Band, Evidence, Facts, Proof, Suggestion};
use crate::ident::GameId;
use crate::{Error, dat, names, systems, titles, view};

/// Header regions as region words: "Japan", "USA", Mega Drive's "JUE".
fn header_regions(id: &GameId) -> Vec<&'static str> {
    let Some(r) = id.region.as_deref() else {
        return Vec::new();
    };
    match r {
        "Japan" => vec!["Japan"],
        "USA" => vec!["USA"],
        "Europe" => vec!["Europe"],
        _ if id.kind == "md" || id.kind == "segacd" || id.kind == "saturn" => r
            .chars()
            .filter_map(|c| match c {
                'J' => Some("Japan"),
                'U' => Some("USA"),
                'E' => Some("Europe"),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn mib(n: u64) -> String {
    if n >= 1 << 20 && n.is_multiple_of(1 << 20) {
        format!("{} MiB", n >> 20)
    } else if n >= 1 << 20 {
        format!("{:.1} MiB", n as f64 / (1u64 << 20) as f64)
    } else {
        format!("{} KiB", n.div_ceil(1024))
    }
}

/// Weigh one candidate (system id, DAT title). `alias`: a name LaunchBox
/// gives the game, alike the file's ([`super::aliases`]).
pub(super) fn assess(
    conn: &Connection,
    f: &Facts,
    proofs: &BTreeMap<(i64, String), Proof>,
    (system_id, title): (i64, &str),
    serial: Option<&str>,
    alias: Option<&str>,
) -> Result<Option<Suggestion>, Error> {
    let mut ev = Vec::new();
    let mut score = titles::score(&f.text, title);
    let name_text = match score {
        s if s >= 1.0 => "the same title".to_owned(),
        s if s >= 0.9 => "the same title, a subtitle missing".to_owned(),
        s if s >= 0.85 => "every word of one in the other".to_owned(),
        s if s > 0.0 => format!("similar title ({:.0}%)", s * 100.0),
        _ => "a different title".to_owned(),
    };
    ev.push(Evidence {
        kind: "name".into(),
        agrees: score >= 0.5,
        text: name_text,
    });
    // Another name of the game, alike the file's, counts as the same title.
    if let Some(a) = alias {
        ev.push(Evidence {
            kind: "alias".into(),
            agrees: true,
            text: format!("LaunchBox gives it the name \"{a}\""),
        });
        score = score.max(0.9);
    }
    let header_title = f.ids.iter().find_map(|i| i.title.clone());
    let header_fits = header_title
        .as_deref()
        .is_some_and(|t| titles::header_fits(t, title));
    if let Some(t) = &header_title {
        ev.push(Evidence {
            kind: "header_title".into(),
            agrees: header_fits,
            text: format!("header title {t}"),
        });
    }
    let rom_sizes: Vec<u64> = conn
        .prepare_cached(
            "SELECT DISTINCT r.size FROM roms r JOIN games g ON g.game_id = r.game_id
             JOIN dats d ON d.dat_id = g.dat_id
             WHERE d.system_id = ?1 AND g.name = ?2 AND r.size IS NOT NULL",
        )?
        .query_map(params![system_id, title], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let same_size = f.sizes.iter().any(|s| rom_sizes.contains(s));
    let compressed = f.ids.iter().any(|i| i.image_size.is_some());
    if let (Some(mine), Some(theirs)) = (f.sizes.first(), rom_sizes.first()) {
        ev.push(Evidence {
            kind: "size".into(),
            agrees: same_size,
            text: match (same_size, compressed) {
                (true, true) => format!("holds an image the size of its ROM ({})", mib(*mine)),
                (true, false) => format!("the same size as its ROM ({})", mib(*mine)),
                (false, true) => format!("holds a {} image, its ROM {}", mib(*mine), mib(*theirs)),
                (false, false) => format!("{} here, its ROM {}", mib(*mine), mib(*theirs)),
            },
        });
    }
    let crc = f.crc.contains(&(system_id, title.to_owned()));
    if crc {
        ev.push(Evidence {
            kind: "crc".into(),
            agrees: true,
            text: "CRC32 agrees (no SHA1 or MD5 to confirm)".into(),
        });
    }
    let patched = proofs.get(&(system_id, title.to_owned())).copied();
    if let Some(proof) = patched {
        ev.push(Evidence {
            kind: "patch".into(),
            agrees: true,
            text: match proof {
                Proof::Applied => {
                    "the patch beside it, applied to your verified copy, gives exactly this file (SHA1)"
                        .into()
                }
                Proof::Turns => {
                    "a patch beside it turns its ROM into this file (sizes and CRC32s agree)".into()
                }
                Proof::MadeFrom => "a patch beside it was made from its ROM (size and CRC32 agree)".into(),
            },
        });
    }
    let nkit = f.nkit.contains(&(system_id, title.to_owned()));
    if nkit {
        ev.push(Evidence {
            kind: "nkit".into(),
            agrees: true,
            text: "its NKit header gives the original image's CRC32, which is its ROM's".into(),
        });
    }
    if let Some(serial) = serial {
        let code = f.ids.iter().find_map(|i| i.serial.clone()).unwrap_or_default();
        ev.push(Evidence {
            kind: "serial".into(),
            agrees: true,
            text: if dat::serial_key(&code) == dat::serial_key(serial) {
                format!("header serial {code} is its serial")
            } else {
                format!("header code {code} is in its serial {serial}")
            },
        });
    }
    let theirs = names::parse(title);
    let header_regions: Vec<&str> = f.ids.iter().flat_map(header_regions).collect();
    let mine = names::parse(&f.own);
    let (mine_regions, from): (Vec<String>, &str) = if !header_regions.is_empty() {
        (header_regions.iter().map(|r| (*r).to_owned()).collect(), "header")
    } else {
        (mine.regions.clone(), "name")
    };
    if !mine_regions.is_empty() && !theirs.regions.is_empty() {
        let world = theirs.regions.iter().any(|r| r == "World");
        let agrees = world
            || mine_regions
                .iter()
                .any(|r| theirs.regions.iter().any(|t| t.eq_ignore_ascii_case(r)));
        ev.push(Evidence {
            kind: "region".into(),
            agrees,
            text: format!(
                "{from} region {} vs {}",
                mine_regions.join(", "),
                theirs.regions.join(", ")
            ),
        });
    }
    let derived = mine
        .translation
        .as_ref()
        .map(|t| (view::Relation::Translation, t))
        .or(mine.hack.as_ref().map(|h| (view::Relation::Hack, h)))
        .map(|(relation, tag)| view::Derived {
            relation,
            base: title.to_owned(),
            tag: Some(tag.clone()).filter(|t| !t.eq_ignore_ascii_case("hack")),
        })
        // Patched from it, the name saying nothing: a hack of it.
        .or_else(|| {
            patched.map(|_| view::Derived {
                relation: view::Relation::Hack,
                base: title.to_owned(),
                tag: None,
            })
        });
    if let Some(d) = &derived {
        ev.push(Evidence {
            kind: "derived".into(),
            agrees: true,
            text: format!("its name says {}", d.relation.as_str())
                + &d.tag.as_ref().map(|t| format!(": {t}")).unwrap_or_default(),
        });
    }
    if !crc {
        ev.push(Evidence {
            kind: "hash".into(),
            agrees: false,
            text: "no SHA1 or MD5 match".into(),
        });
    }

    let band = if patched == Some(Proof::Applied) {
        Band::PatchOf
    } else if crc && same_size {
        Band::LikelyCrc
    } else if (compressed && same_size || nkit) && serial.is_some() {
        Band::LikelyCompressed
    } else if patched == Some(Proof::Turns)
        || (same_size
            && (score >= 0.9
                || serial.is_some()
                // A translation or hack may be expanded: its size only
                // backs a title that is the same.
                || (score >= 0.85 && header_fits && derived.is_none())))
    {
        Band::LikelyModified
    } else if score >= 0.9 || (serial.is_some() && (score >= 0.5 || header_fits)) {
        Band::Suggested
    } else if score >= 0.5 || serial.is_some() || crc || nkit {
        Band::Possible
    } else {
        return Ok(None);
    };
    Ok(Some(Suggestion {
        system: systems::by_id(system_id).map_or(String::new(), |s| s.key.to_owned()),
        title: title.to_owned(),
        band,
        score,
        evidence: ev,
        derived,
    }))
}
