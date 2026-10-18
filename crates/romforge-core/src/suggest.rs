//! Suggestions for files no hash identified, each with its evidence
//! (milestone 12.6).
//!
//! Candidates come from three places: a CRC32 that matched without a SHA1
//! or MD5, a game ID read from the file's own header that a DAT game's
//! serial carries, and titles alike to the file's name or its header
//! title. Each candidate is then weighed on everything known: name, header
//! title, size, CRC, serial, region. The band says how strong that is; no
//! band is ever counted or shown as verified (house rule 8). Confirming a
//! suggestion makes the file "identified by you".

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension, params};

use crate::ident::GameId;
use crate::{Error, dat, names, systems, titles, view};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    /// A patch beside it, applied to your hash-verified copy of the game,
    /// gives exactly this file: a proven patch of it (still never
    /// verified: the file itself is in no DAT).
    PatchOf,
    /// CRC32 and size agree, but the DAT's SHA1 or MD5 doesn't (or it has
    /// none).
    LikelyCrc,
    /// A compressed disc image (RVZ, WIA, GCZ, CSO, ZSO) holding an image
    /// of the game's size, its ID in the game's serial. Only decompressing
    /// it all could verify it.
    LikelyCompressed,
    /// The same size as the game's ROM and its name or serial agrees: a bad
    /// dump, a hack or a translation of it.
    LikelyModified,
    /// The same title (or its serial with a title that fits).
    Suggested,
    /// A similar title, or a serial alone.
    Possible,
}

impl Band {
    pub fn label(self) -> &'static str {
        match self {
            Band::PatchOf => "Patch of it (proven)",
            Band::LikelyCrc => "Likely: CRC match",
            Band::LikelyCompressed => "Likely: compressed image",
            Band::LikelyModified => "Likely: modified copy",
            Band::Suggested => "Suggested: name match",
            Band::Possible => "Possible: similar name",
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Evidence {
    /// `name`, `header_title`, `size`, `crc`, `serial`, `region`, `hash`.
    pub kind: String,
    /// For or against.
    pub agrees: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Suggestion {
    /// System key.
    pub system: String,
    /// The DAT game's name.
    pub title: String,
    pub band: Band,
    /// How alike the names are ([`titles::score`]), 0 to 1.
    pub score: f32,
    pub evidence: Vec<Evidence>,
    /// When the file's name says translation or hack: taking it names the
    /// file so ("translation of X"), never verified.
    pub derived: Option<view::Derived>,
}

/// How much a patch beside a file says about a DAT game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Proof {
    /// BPS / UPS: made from a ROM of its size and CRC32.
    MadeFrom,
    /// ... and makes a file of this one's CRC32.
    Turns,
    /// Applied to your hash-verified copy, it gives exactly this file
    /// (SHA1).
    Applied,
}

/// What is known about a row's files.
struct Facts {
    /// What the titles are scored against: what the owner typed, or `own`.
    text: String,
    /// The row's own title or file name, which alone says what the file is
    /// (a translation, a region): typed text only searches.
    own: String,
    sizes: Vec<u64>,
    ids: Vec<GameId>,
    systems: Vec<i64>,
    /// (system id, DAT game) a CRC32 hit.
    crc: BTreeSet<(i64, String)>,
    /// (system id, DAT game) a patch beside a file was made from.
    patched: BTreeMap<(i64, String), Proof>,
    /// Patches beside the files that name no source (IPS), with the SHA1
    /// of the file beside them, to try on verified copies of candidates.
    unnamed: Vec<(Vec<u8>, String)>,
    /// (system id, DAT game) whose ROM has the CRC32 an NKit image gives
    /// for its original image.
    nkit: BTreeSet<(i64, String)>,
}

/// Your hash-verified copy of a DAT game's (single) ROM, if any.
fn verified_copy(conn: &Connection, system_id: i64, title: &str) -> Result<Option<(String, String)>, Error> {
    Ok(conn
        .prepare_cached(
            "SELECT f.path, f.member FROM files f JOIN matches m ON m.file_id = f.file_id
             JOIN roms r ON r.rom_id = m.rom_id JOIN games g ON g.game_id = r.game_id
             JOIN dats d ON d.dat_id = g.dat_id
             WHERE d.system_id = ?1 AND g.name = ?2 AND m.via IN ('sha1', 'md5') AND m.stripped = 0
               AND f.status IN ('matched', 'duplicate')
               AND (SELECT count(*) FROM roms x WHERE x.game_id = g.game_id) = 1
             LIMIT 1",
        )?
        .query_row(params![system_id, title], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?)
}

/// Whether `patch` applied to your verified copy of the game gives exactly
/// the file with `sha1`.
fn applies(conn: &Connection, patch: &[u8], system_id: i64, title: &str, sha1: &str) -> Result<bool, Error> {
    let Some((path, member)) = verified_copy(conn, system_id, title)? else {
        return Ok(false);
    };
    let Some(base) = crate::patch::read_entry(std::path::Path::new(&path), &member) else {
        return Ok(false);
    };
    let Some(out) = crate::patch::apply(patch, &base) else {
        return Ok(false);
    };
    Ok(crate::hash::hash_stream(&out[..], out.len() as u64)?.raw.sha1 == sha1)
}

fn facts(
    conn: &Connection,
    d: &view::GameDetail,
    text: Option<&str>,
    system: Option<&str>,
    patches: &mut crate::patch::PatchFinder,
) -> Result<Facts, Error> {
    let own = crate::search::default_text(d);
    let text = text
        .filter(|t| !t.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| own.clone());
    // A compressed disc image counts by the plain image it holds.
    let sizes: Vec<u64> = d
        .files
        .iter()
        .map(|f| f.game_id.as_ref().and_then(|i| i.image_size).unwrap_or(f.size))
        .collect();
    let ids: Vec<GameId> = d.files.iter().filter_map(|f| f.game_id.clone()).collect();
    let mut systems: Vec<i64> = match system.or(d.row.system.as_deref()) {
        Some(key) => systems::by_key(key).map(|s| vec![s.id]).unwrap_or_default(),
        None => Vec::new(),
    };
    if systems.is_empty() {
        // The header's kind first, then systems using the extension.
        systems.extend(ids.iter().flat_map(|i| systems::by_ident(i.kind)).map(|s| s.id));
        let ext = d.files.first().and_then(|f| {
            std::path::Path::new(f.path.rsplit('#').next().unwrap_or(&f.path))
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
        });
        if let Some(ext) = ext {
            systems.extend(
                systems::SYSTEMS
                    .iter()
                    .filter(|s| s.extensions.contains(&ext.as_str()))
                    .map(|s| s.id),
            );
        }
        let mut seen = BTreeSet::new();
        systems.retain(|s| seen.insert(*s));
    }
    let mut crc = BTreeSet::new();
    let mut stmt = conn.prepare_cached(
        "SELECT d.system_id, g.name FROM matches m JOIN roms r ON r.rom_id = m.rom_id
         JOIN games g ON g.game_id = r.game_id JOIN dats d ON d.dat_id = g.dat_id
         WHERE m.file_id = ?1 AND m.via = 'crc32' AND d.system_id IS NOT NULL",
    )?;
    for id in d.row.file_ids() {
        for r in stmt.query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
            crc.insert(r?);
        }
    }
    // Patches beside the files ("Game.sfc" and "Game.bps"), on disk or in
    // the same archive.
    let mut patched = BTreeMap::new();
    let mut unnamed = Vec::new();
    let mut file = conn.prepare_cached("SELECT path, member, crc32, sha1 FROM files WHERE file_id = ?1")?;
    let mut roms = conn.prepare_cached(
        "SELECT DISTINCT d.system_id, g.name FROM roms r JOIN games g ON g.game_id = r.game_id
         JOIN dats d ON d.dat_id = g.dat_id
         WHERE r.crc32 = ?1 AND r.size = ?2 AND d.system_id IS NOT NULL",
    )?;
    for id in d.row.file_ids() {
        let (path, member, crc, sha1): (String, String, String, String) =
            file.query_row([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        for data in patches.beside(std::path::Path::new(&path), &member) {
            let Some(info) = crate::patch::info(&data) else {
                continue;
            };
            let Some((size, source_crc)) = info.source else {
                unnamed.push((data, sha1.clone()));
                continue;
            };
            let makes_it = info.target.is_some_and(|(_, t)| format!("{t:08x}") == crc);
            let games: Vec<(i64, String)> = roms
                .query_map(params![format!("{source_crc:08x}"), size], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })?
                .collect::<Result<_, _>>()?;
            for (sid, title) in games {
                let proof = if makes_it && applies(conn, &data, sid, &title, &sha1)? {
                    Proof::Applied
                } else if makes_it {
                    Proof::Turns
                } else {
                    Proof::MadeFrom
                };
                let e = patched.entry((sid, title)).or_insert(proof);
                *e = (*e).max(proof);
            }
        }
    }
    // NKit: the original image's CRC32, byte order undocumented, so both.
    let mut nkit = BTreeSet::new();
    let mut by_crc = conn.prepare_cached(
        "SELECT DISTINCT d.system_id, g.name FROM roms r JOIN games g ON g.game_id = r.game_id
         JOIN dats d ON d.dat_id = g.dat_id WHERE r.crc32 IN (?1, ?2) AND d.system_id IS NOT NULL",
    )?;
    for c in ids.iter().filter_map(|i| i.source_crc) {
        let (be, le) = (format!("{c:08x}"), format!("{:08x}", c.swap_bytes()));
        for r in by_crc.query_map(params![be, le], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })? {
            let r = r?;
            if systems.is_empty() || systems.contains(&r.0) {
                nkit.insert(r);
            }
        }
    }
    Ok(Facts {
        text,
        own,
        sizes,
        ids,
        systems,
        crc,
        patched,
        unnamed,
        nkit,
    })
}

/// DAT games whose serial carries one of the file's IDs, with that serial.
fn by_serial(conn: &Connection, f: &Facts) -> Result<BTreeMap<(i64, String), String>, Error> {
    let mut out = BTreeMap::new();
    let mut stmt = conn.prepare_cached(
        "SELECT system_id, title, serial FROM serials WHERE system_id = ?1
           AND (key = ?2 OR (length(?2) = 4 AND instr(key, ?2) > 0))",
    )?;
    for id in &f.ids {
        let Some(serial) = &id.serial else { continue };
        let mut key = dat::serial_key(serial);
        // GameCube / Wii: "GALE01" is game code GALE and maker 01; Redump's
        // serials ("DL-DOL-GALE-USA") carry the game code only.
        if matches!(id.kind, "gc" | "wii") {
            key.truncate(4);
        }
        if key.len() < 4 {
            continue;
        }
        for s in systems::by_ident(id.kind).filter(|s| f.systems.contains(&s.id)) {
            for r in stmt.query_map(params![s.id, key], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })? {
                let (sid, title, serial) = r?;
                // A short game code must be a whole part of the serial
                // ("AQ3J" in SHVC-AQ3J-JPN), not letters across two parts.
                if dat::serial_key(&serial) != key && !code_in(&key, &serial) {
                    continue;
                }
                out.entry((sid, title)).or_insert(serial);
            }
        }
    }
    Ok(out)
}

/// Whether `code` (letters and digits) is one of `serial`'s parts, split at
/// anything else ("SHVC-AQ3J-JPN" has AQ3J).
fn code_in(code: &str, serial: &str) -> bool {
    serial
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case(code))
}

/// The DAT names of `systems`, each with its systems.
fn dat_names(conn: &Connection, systems: &[i64]) -> Result<BTreeMap<String, Vec<i64>>, Error> {
    let mut out: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    let ids = serde_json::to_string(systems).unwrap_or_else(|_| "[]".into());
    let mut stmt = conn.prepare(
        "SELECT DISTINCT d.system_id, g.name FROM games g JOIN dats d ON d.dat_id = g.dat_id
         WHERE d.system_id IS NOT NULL AND (?1 = 0 OR d.system_id IN (SELECT value FROM json_each(?2)))",
    )?;
    for r in stmt.query_map(params![systems.len() as i64, ids], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })? {
        let (sid, name) = r?;
        out.entry(name).or_default().push(sid);
    }
    Ok(out)
}

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

/// Weigh one candidate.
fn assess(
    conn: &Connection,
    f: &Facts,
    proofs: &BTreeMap<(i64, String), Proof>,
    system_id: i64,
    title: &str,
    serial: Option<&str>,
) -> Result<Option<Suggestion>, Error> {
    let mut ev = Vec::new();
    let score = titles::score(&f.text, title);
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
        || (same_size && (score >= 0.9 || serial.is_some() || (score >= 0.85 && header_fits)))
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

/// Suggestions for a row, strongest first: by band, then how much of the
/// evidence agrees, then the name. `text`: what the owner typed (the
/// row's title or file name when empty); `system`: limit to this key.
pub(crate) fn for_detail(
    conn: &Connection,
    d: &view::GameDetail,
    text: Option<&str>,
    system: Option<&str>,
    limit: usize,
) -> Result<Vec<Suggestion>, Error> {
    Suggester::default().suggest(conn, d, text, system, limit)
}

/// DAT names (each with its systems) and their title index.
type Names = (BTreeMap<String, Vec<i64>>, titles::TitleIndex);

/// Suggestions for many rows, keeping each set of systems' title index.
#[derive(Default)]
pub struct Suggester {
    indexes: BTreeMap<Vec<i64>, Names>,
    /// Folders and archives listed for patches, once each.
    patches: crate::patch::PatchFinder,
}

impl Suggester {
    pub fn suggest(
        &mut self,
        conn: &Connection,
        d: &view::GameDetail,
        text: Option<&str>,
        system: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Suggestion>, Error> {
        let f = facts(conn, d, text, system, &mut self.patches)?;
        if !self.indexes.contains_key(&f.systems) {
            let names = dat_names(conn, &f.systems)?;
            let index = titles::TitleIndex::new(names.keys().cloned());
            self.indexes.insert(f.systems.clone(), (names, index));
        }
        let (names, index) = &self.indexes[&f.systems];
        suggest_with(conn, &f, names, index, limit)
    }
}

fn suggest_with(
    conn: &Connection,
    f: &Facts,
    names: &BTreeMap<String, Vec<i64>>,
    index: &titles::TitleIndex,
    limit: usize,
) -> Result<Vec<Suggestion>, Error> {
    let serials = by_serial(conn, f)?;
    let mut candidates: BTreeSet<(i64, String)> = f.crc.clone();
    candidates.extend(f.patched.keys().cloned());
    candidates.extend(f.nkit.iter().cloned());
    candidates.extend(serials.keys().cloned());
    let header_titles: Vec<String> = f.ids.iter().filter_map(|i| i.title.clone()).collect();
    for q in std::iter::once(&f.text).chain(&header_titles) {
        for (title, _) in index.find(q, 40) {
            for sid in &names[&title] {
                candidates.insert((*sid, title.clone()));
            }
        }
    }
    // IPS names no source: try it on your verified copies of the
    // candidates (a few, best named first).
    let mut proofs = f.patched.clone();
    if !f.unnamed.is_empty() {
        let mut tried: Vec<&(i64, String)> = candidates.iter().collect();
        tried.sort_by(|a, b| titles::score(&f.text, &b.1).total_cmp(&titles::score(&f.text, &a.1)));
        for (sid, title) in tried.into_iter().take(20) {
            for (data, sha1) in &f.unnamed {
                if applies(conn, data, *sid, title, sha1)? {
                    proofs.insert((*sid, title.clone()), Proof::Applied);
                }
            }
        }
    }
    let mut out = Vec::new();
    for (sid, title) in &candidates {
        if !f.systems.is_empty() && !f.systems.contains(sid) {
            continue;
        }
        let serial = serials.get(&(*sid, title.clone())).map(String::as_str);
        if let Some(s) = assess(conn, f, &proofs, *sid, title, serial)? {
            out.push(s);
        }
    }
    let agreeing = |s: &Suggestion| s.evidence.iter().filter(|e| e.agrees).count();
    out.sort_by(|a, b| {
        a.band
            .cmp(&b.band)
            .then(agreeing(b).cmp(&agreeing(a)))
            .then(b.score.total_cmp(&a.score))
            .then_with(|| a.title.cmp(&b.title))
    });
    out.truncate(limit);
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn codes_are_whole_parts_of_serials() {
        use super::code_in;
        assert!(code_in("AQ3J", "SHVC-AQ3J-JPN"));
        assert!(code_in("GALE", "DL-DOL-GALE-USA"));
        assert!(!code_in("CAQ3", "SHVC-AQ3J-JPN"), "across two parts");
        assert!(!code_in("AQ3J", "SHVCAQ3JJPN"), "no parts to tell");
    }
}
