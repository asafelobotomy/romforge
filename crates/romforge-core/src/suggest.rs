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

use rusqlite::{Connection, params};

use crate::ident::GameId;
use crate::{Error, dat, names, systems, titles, view};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    /// CRC32 and size agree, but the DAT's SHA1 or MD5 doesn't (or it has
    /// none).
    LikelyCrc,
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
            Band::LikelyCrc => "Likely: CRC match",
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

/// What a BPS or UPS patch says about what it was made from and makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Patch {
    pub source_size: u64,
    pub target_size: u64,
    pub source_crc: u32,
    pub target_crc: u32,
}

/// Read a BPS or UPS patch: sizes from its header, CRC32s from its footer
/// (the patch's own CRC32 must check out). IPS stores no checksums.
pub fn read_patch(data: &[u8]) -> Option<Patch> {
    if data.len() < 4 + 12 || !(data.starts_with(b"BPS1") || data.starts_with(b"UPS1")) {
        return None;
    }
    let (body, footer) = data.split_at(data.len() - 12);
    let le = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    if crc32fast::hash(&data[..data.len() - 4]) != le(&footer[8..]) {
        return None;
    }
    // Their variable-length numbers: 7 bits a byte, the high bit ends it.
    let mut at = 4;
    let mut number = || -> Option<u64> {
        let (mut value, mut shift) = (0u64, 1u64);
        loop {
            let b = *body.get(at)?;
            at += 1;
            value = value.checked_add((b & 0x7f) as u64 * shift)?;
            if b & 0x80 != 0 {
                return Some(value);
            }
            shift = shift.checked_shl(7)?;
            value = value.checked_add(shift)?;
        }
    };
    let source_size = number()?;
    let target_size = number()?;
    Some(Patch {
        source_size,
        target_size,
        source_crc: le(&footer[..4]),
        target_crc: le(&footer[4..8]),
    })
}

/// What is known about a row's files.
struct Facts {
    text: String,
    sizes: Vec<u64>,
    ids: Vec<GameId>,
    systems: Vec<i64>,
    /// (system id, DAT game) a CRC32 hit.
    crc: BTreeSet<(i64, String)>,
    /// (system id, DAT game) a patch beside a file was made from, and
    /// whether the patch makes that file.
    patched: BTreeMap<(i64, String), bool>,
}

fn facts(
    conn: &Connection,
    d: &view::GameDetail,
    text: Option<&str>,
    system: Option<&str>,
) -> Result<Facts, Error> {
    let text = text
        .filter(|t| !t.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| crate::search::default_text(d));
    let sizes: Vec<u64> = d.files.iter().map(|f| f.size).collect();
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
    // Patches beside plain files ("Game.sfc" and "Game.bps" or ".ups").
    let mut patched = BTreeMap::new();
    let mut file = conn.prepare_cached("SELECT path, member, crc32 FROM files WHERE file_id = ?1")?;
    let mut roms = conn.prepare_cached(
        "SELECT DISTINCT d.system_id, g.name FROM roms r JOIN games g ON g.game_id = r.game_id
         JOIN dats d ON d.dat_id = g.dat_id
         WHERE r.crc32 = ?1 AND r.size = ?2 AND d.system_id IS NOT NULL",
    )?;
    for id in d.row.file_ids() {
        let (path, member, crc): (String, String, String) =
            file.query_row([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        if !member.is_empty() {
            continue;
        }
        for ext in ["bps", "ups", "BPS", "UPS"] {
            let Ok(data) = std::fs::read(std::path::Path::new(&path).with_extension(ext)) else {
                continue;
            };
            let Some(p) = read_patch(&data) else { continue };
            let makes_it = format!("{:08x}", p.target_crc) == crc;
            for r in roms.query_map(params![format!("{:08x}", p.source_crc), p.source_size], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })? {
                patched.insert(r?, makes_it);
            }
        }
    }
    Ok(Facts {
        text,
        sizes,
        ids,
        systems,
        crc,
        patched,
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
        let key = dat::serial_key(serial);
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
                out.entry((sid, title)).or_insert(serial);
            }
        }
    }
    Ok(out)
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
    if let (Some(mine), Some(theirs)) = (f.sizes.first(), rom_sizes.first()) {
        ev.push(Evidence {
            kind: "size".into(),
            agrees: same_size,
            text: if same_size {
                format!("the same size as its ROM ({})", mib(*mine))
            } else {
                format!("{} here, its ROM {}", mib(*mine), mib(*theirs))
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
    let patched = f.patched.get(&(system_id, title.to_owned())).copied();
    if let Some(makes_it) = patched {
        ev.push(Evidence {
            kind: "patch".into(),
            agrees: true,
            text: if makes_it {
                "a patch beside it turns its ROM into this file (sizes and CRC32s agree)".into()
            } else {
                "a patch beside it was made from its ROM (size and CRC32 agree)".into()
            },
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
    let mine = names::parse(&f.text);
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

    let band = if crc && same_size {
        Band::LikelyCrc
    } else if patched == Some(true) || (same_size && (score >= 0.85 || serial.is_some())) {
        Band::LikelyModified
    } else if score >= 0.9 || (serial.is_some() && (score >= 0.5 || header_fits)) {
        Band::Suggested
    } else if score >= 0.5 || serial.is_some() || crc {
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
pub fn for_row(
    conn: &Connection,
    row_id: &str,
    text: Option<&str>,
    system: Option<&str>,
    limit: usize,
) -> Result<Vec<Suggestion>, Error> {
    let d = view::detail(conn, row_id)?;
    for_detail(conn, &d, text, system, limit)
}

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
        let f = facts(conn, d, text, system)?;
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
    candidates.extend(serials.keys().cloned());
    let header_titles: Vec<String> = f.ids.iter().filter_map(|i| i.title.clone()).collect();
    for q in std::iter::once(&f.text).chain(&header_titles) {
        for (title, _) in index.find(q, 40) {
            for sid in &names[&title] {
                candidates.insert((*sid, title.clone()));
            }
        }
    }
    let mut out = Vec::new();
    for (sid, title) in &candidates {
        if !f.systems.is_empty() && !f.systems.contains(sid) {
            continue;
        }
        let serial = serials.get(&(*sid, title.clone())).map(String::as_str);
        if let Some(s) = assess(conn, f, *sid, title, serial)? {
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
    use super::*;

    /// BPS's variable-length number.
    fn number(mut n: u64, out: &mut Vec<u8>) {
        loop {
            let x = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(0x80 | x);
                return;
            }
            out.push(x);
            n -= 1;
        }
    }

    pub(crate) fn bps(source: &[u8], target: &[u8]) -> Vec<u8> {
        let mut p = b"BPS1".to_vec();
        number(source.len() as u64, &mut p);
        number(target.len() as u64, &mut p);
        number(0, &mut p); // no metadata
        p.extend_from_slice(&crc32fast::hash(source).to_le_bytes());
        p.extend_from_slice(&crc32fast::hash(target).to_le_bytes());
        let own = crc32fast::hash(&p);
        p.extend_from_slice(&own.to_le_bytes());
        p
    }

    #[test]
    fn reads_bps_sizes_and_crcs() {
        let (source, target) = (vec![1u8; 300_000], vec![2u8; 70]);
        let p = read_patch(&bps(&source, &target)).unwrap();
        assert_eq!((p.source_size, p.target_size), (300_000, 70));
        assert_eq!(p.source_crc, crc32fast::hash(&source));
        assert_eq!(p.target_crc, crc32fast::hash(&target));
        let mut broken = bps(&source, &target);
        broken[5] ^= 1;
        assert_eq!(read_patch(&broken), None, "its own CRC32 must check out");
        assert_eq!(read_patch(b"PATCH...EOF"), None, "IPS has no checksums");
    }
}
