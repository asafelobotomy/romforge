use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::organize::sanitize;
use crate::view::{self, RowStatus};
use crate::{Error, layout};

use super::{IN_MEMORY, Part, Source};

/// (the ROMs to write, other copies of them in Source)
pub(crate) type PartsAndCopies = (Vec<Part>, Vec<Part>);

/// The ROMs of a hash-verified game: one file per ROM (the copy in Source
/// first, a matched file before a duplicate), from the DAT that has them
/// all, and the other copies of them in Source. Cue sheets are taken when
/// matched; a game is complete without one (a CHD needs none).
pub(crate) fn verified_parts(
    conn: &Connection,
    system_id: i64,
    title: &str,
    source: &Source,
) -> Result<Option<PartsAndCopies>, Error> {
    let mut stmt = conn.prepare(concat!(
        "SELECT g.game_id, r.rom_id, r.name, f.path, f.member, f.size, f.sha1, f.status, m.owns
         FROM games g JOIN dats d ON d.dat_id = g.dat_id
         JOIN roms r ON r.game_id = g.game_id
         LEFT JOIN matches m ON m.rom_id = r.rom_id AND m.out_of_scope = 0 AND m.via IN ",
        crate::report::strong!(),
        "
         LEFT JOIN files f ON f.file_id = m.file_id AND f.status IN ",
        crate::report::holding!(),
        "
         WHERE d.system_id = ?1 AND g.name = ?2 AND coalesce(r.status, '') NOT IN ('nodump', 'optional')
         ORDER BY g.game_id, r.rom_id, f.path, f.member"
    ))?;
    // game -> rom -> (DAT name, every copy with its rank: lower is better)
    type Copies = (String, Vec<(Part, u8)>);
    let mut games: BTreeMap<i64, BTreeMap<i64, Copies>> = BTreeMap::new();
    for r in stmt.query_map(params![system_id, title], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<u64>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<String>>(7)?,
            r.get::<_, Option<bool>>(8)?,
        ))
    })? {
        let (game, rom, name, path, member, size, sha1, status, owns) = r?;
        let entry = games
            .entry(game)
            .or_default()
            .entry(rom)
            .or_insert((name.clone(), Vec::new()));
        if let (Some(path), Some(member), Some(size), Some(sha1), Some(status)) =
            (path, member, size, sha1, status)
        {
            let path = PathBuf::from(path);
            // In Source first, then the copy that owns the ROM (beside the
            // game's other files), then a matched file before a duplicate.
            let rank = (!source.contains(&path)) as u8 * 4
                + (owns != Some(true)) as u8 * 2
                + (status != "matched") as u8;
            let part = Part {
                name,
                path,
                member,
                size,
                sha1,
            };
            entry.1.push((part, rank));
        }
    }
    for roms in games.values() {
        let complete = roms
            .values()
            .filter(|(n, _)| !n.to_lowercase().ends_with(".cue"))
            .all(|(_, c)| !c.is_empty());
        if !complete || roms.values().all(|(_, c)| c.is_empty()) {
            continue;
        }
        let (mut parts, mut copies) = (Vec::new(), Vec::new());
        for (_, c) in roms.values() {
            let Some(best) = c
                .iter()
                .enumerate()
                .min_by_key(|(_, (_, rank))| *rank)
                .map(|(i, _)| i)
            else {
                continue;
            };
            for (i, (p, _)) in c.iter().enumerate() {
                if i == best {
                    parts.push(p.clone());
                } else if source.contains(&p.path) {
                    copies.push(p.clone());
                }
            }
        }
        return Ok(Some((parts, copies)));
    }
    Ok(None)
}

/// The files of a game you named, under their own names.
/// The files of a game you named: one per content (other copies of it in
/// Source are `copies`, the one in Source first), tracks in natural order
/// ("Track 2" before "Track 10") as there is no DAT to give it.
pub(crate) fn named_parts(
    conn: &Connection,
    row_id: &str,
    source: &Source,
) -> Result<Option<PartsAndCopies>, Error> {
    let d = view::detail(conn, row_id)?;
    let mut all: Vec<Part> = d
        .files
        .iter()
        .map(|f| {
            let (path, member) = match f.path.split_once('#') {
                Some((p, m)) => (PathBuf::from(p), m.to_owned()),
                None => (PathBuf::from(&f.path), String::new()),
            };
            let shown = if member.is_empty() {
                path.to_string_lossy().into_owned()
            } else {
                member.clone()
            };
            Part {
                name: shown.rsplit('/').next().unwrap_or(&shown).to_owned(),
                path,
                member,
                size: f.size,
                sha1: f.sha1.clone(),
            }
        })
        .collect();
    all.sort_by_key(|p| (!source.contains(&p.path), natural_key(&p.name)));
    let (mut parts, mut copies) = (Vec::new(), Vec::new());
    let mut seen = BTreeSet::new();
    for p in all {
        if seen.insert((p.sha1.clone(), p.size)) {
            parts.push(p);
        } else if source.contains(&p.path) {
            copies.push(p);
        }
    }
    parts.sort_by_key(|p| natural_key(&p.name));
    Ok((!parts.is_empty()).then_some((parts, copies)))
}

/// A name for sorting with its numbers as numbers.
pub(super) fn natural_key(name: &str) -> Vec<(String, u64)> {
    let lower = name.to_lowercase();
    let mut out = Vec::new();
    let mut text = String::new();
    let mut chars = lower.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            let mut n = c.to_digit(10).unwrap_or(0) as u64;
            while let Some(d) = chars.peek().and_then(|d| d.to_digit(10)) {
                n = n.saturating_mul(10).saturating_add(d as u64);
                chars.next();
            }
            out.push((std::mem::take(&mut text), n));
        } else {
            text.push(c);
        }
    }
    out.push((text, 0));
    out
}

/// Open a part's data as a stream: a plain file or a zip member directly;
/// a 7z member is unpacked into `staging` first (one compressed stream).
pub(crate) fn open_part(part: &Part, staging: &Path) -> Result<Box<dyn Read>, Error> {
    if part.member.is_empty() {
        return Ok(Box::new(BufReader::new(File::open(&part.path)?)));
    }
    let ext = part
        .path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    match ext.as_deref() {
        Some("zip") => {
            let mut zip = zip::ZipArchive::new(BufReader::new(File::open(&part.path)?))?;
            let mut m = zip.by_name(&part.member)?;
            if m.size() <= IN_MEMORY {
                let mut buf = Vec::new();
                m.read_to_end(&mut buf)?;
                return Ok(Box::new(std::io::Cursor::new(buf)));
            }
            // A big member (a disc track) is unpacked to staging instead.
            std::fs::create_dir_all(staging)?;
            let out = staging.join(format!("{}.part", part.sha1));
            std::io::copy(&mut m, &mut File::create(&out)?)?;
            Ok(Box::new(BufReader::new(File::open(out)?)))
        }
        Some("7z") => {
            let out = staging.join(format!("{}.part", part.sha1));
            if !out.exists() {
                std::fs::create_dir_all(staging)?;
                let mut archive =
                    sevenz_rust2::ArchiveReader::open(&part.path, sevenz_rust2::Password::empty())?;
                let mut found = false;
                archive.for_each_entries(|entry, reader| {
                    if entry.name() == part.member {
                        let mut f = File::create(&out)?;
                        std::io::copy(reader, &mut f)?;
                        found = true;
                        return Ok(false);
                    }
                    Ok(true)
                })?;
                if !found {
                    return Err(Error::Cleanup(format!(
                        "{} has no {}",
                        part.path.display(),
                        part.member
                    )));
                }
            }
            Ok(Box::new(BufReader::new(File::open(out)?)))
        }
        _ => Err(Error::Cleanup(format!(
            "can't read {}#{}",
            part.path.display(),
            part.member
        ))),
    }
}

/// What goes in the `romforge/` folder (or sidecar): (file name, bytes).
pub(super) fn extras(conn: &Connection, row_id: &str) -> Result<Vec<(String, Vec<u8>)>, Error> {
    let d = view::detail(conn, row_id)?;
    let mut out = vec![(
        format!("{}{}", sanitize(&d.row.title), layout::DETAILS_SUFFIX),
        details_md(&d).into_bytes(),
    )];
    for (kind, path) in &d.art {
        if let Ok(data) = std::fs::read(path) {
            let ext = crate::enrich::image_ext(&data).unwrap_or("png");
            out.push((format!("{kind}.{ext}"), data));
        }
    }
    Ok(out)
}

/// The game's details as Markdown.
pub fn details_md(d: &view::GameDetail) -> String {
    let mut s = format!("# {}\n\n", d.row.title);
    let derived;
    let status = match d.row.status {
        RowStatus::Verified => "verified by hash",
        RowStatus::ByYou => "identified by you (no hash match)",
        RowStatus::Derivative => {
            derived = d.derived.as_ref().map_or_else(
                || "a derivative, identified by you (no hash match)".to_owned(),
                |x| format!("{}, identified by you (no hash match)", x.label()),
            );
            &derived
        }
        RowStatus::Detected => "identified by ScummVM's detection list",
        _ => "not identified",
    };
    s += &format!(
        "- Platform: {}\n- Status: {status}\n",
        d.row.system_name.as_deref().unwrap_or("?")
    );
    if !d.dats.is_empty() {
        s += &format!("- DAT: {}\n", d.dats.join(", "));
    }
    let show: [(&str, &str); 12] = [
        ("Released", "releaseyear"),
        ("Genre", "genre"),
        ("Players", "users"),
        ("Developer", "developer"),
        ("Publisher", "publisher"),
        ("Country", "country"),
        ("Director", "director"),
        ("Designer", "designer"),
        ("Composer", "composer"),
        ("Series", "series"),
        ("Franchise", "franchise"),
        ("Serial", "serial"),
    ];
    for (label, key) in show {
        if let Some(v) = d.fields.get(key) {
            s += &format!("- {label}: {v}\n");
        }
    }
    if let Some(desc) = d.fields.get("description") {
        s += &format!("\n{desc}\n");
        if let Some(db) = d
            .field_sources
            .get("description")
            .and_then(|k| crate::extdb::ExtDb::parse(k))
        {
            s += &format!("\n_Overview from {}._\n", db.name());
        }
    }
    s += "\n## Files\n\n| File | Size | SHA1 |\n|---|---:|---|\n";
    for f in &d.files {
        let name = f.path.rsplit(['/', '#']).next().unwrap_or(&f.path);
        s += &format!("| {name} | {} | `{}` |\n", f.size, f.sha1);
    }
    let direct: Vec<&view::Link> = d.links.iter().filter(|l| l.direct).collect();
    if !direct.is_empty() {
        s += "\n## Links\n\n";
        for l in direct {
            s += &format!("- [{}]({})\n", l.label, l.url);
        }
    }
    s += "\n_Written by romforge._\n";
    s
}
