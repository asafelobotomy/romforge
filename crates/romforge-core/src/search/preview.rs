//! The preview of a result, and taking one for a row.

use std::collections::BTreeSet;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use super::{Found, Scraper, identify_detail, settle_for};
use crate::view;
use crate::{Error, systems, wikidata};

/// What the preview pane shows for a result.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Preview {
    pub title: String,
    pub year: Option<i32>,
    pub system_name: Option<String>,
    pub description: Option<String>,
    /// (label, value) lines: DAT, ROMs and how they compare with your file.
    pub facts: Vec<(String, String)>,
    /// Box art: from your art cache, else libretro's thumbnail server
    /// (cached under `<cache>/preview`).
    pub image: Option<std::path::PathBuf>,
}

/// (DAT, ROM name, size, CRC32, whether the CRC32 is all it has)
type DatRom = (String, String, Option<u64>, Option<String>, bool);

/// The preview of a result for a row of `v` (the library's list). Never
/// fails over a missing picture.
pub fn preview(
    conn: &Connection,
    v: &view::LibraryView,
    row_id: &str,
    found: &Found,
    cache_dir: &Path,
    thumbnails_base: &str,
) -> Result<Preview, Error> {
    let d = view::detail_in(conn, v, row_id)?;
    let system = found.system.as_deref().and_then(systems::by_key);
    let mut p = Preview {
        title: found.title.clone(),
        year: found.year,
        system_name: system.map(|s| s.name.to_owned()),
        description: found.description.clone(),
        ..Preview::default()
    };
    match found.scraper {
        Scraper::Dat => {
            let Some(sys) = system else { return Ok(p) };
            let mut stmt = conn.prepare(
                "SELECT d.name, r.name, r.size, r.crc32, r.md5 IS NULL AND r.sha1 IS NULL
                 FROM games g JOIN dats d ON d.dat_id = g.dat_id
                 JOIN roms r ON r.game_id = g.game_id
                 WHERE d.system_id = ?1 AND g.name = ?2 ORDER BY d.dat_id, r.rom_id",
            )?;
            let rows: Vec<DatRom> = stmt
                .query_map(params![sys.id, found.title], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?
                .collect::<Result<_, _>>()?;
            let dats: BTreeSet<&str> = rows.iter().map(|r| r.0.as_str()).collect();
            if !dats.is_empty() {
                p.facts
                    .push(("DAT".into(), dats.into_iter().collect::<Vec<_>>().join(", ")));
            }
            let mut seen = BTreeSet::new();
            for (_, name, size, _, _) in &rows {
                if seen.insert(name) {
                    let size = size.map_or("?".into(), fmt_bytes);
                    p.facts.push(("DAT ROM".into(), format!("{name} · {size}")));
                }
            }
            let mut crc_of = conn.prepare("SELECT crc32 FROM files WHERE sha1 = ?1 AND size = ?2 LIMIT 1")?;
            for f in &d.files {
                let file = f.path.rsplit(['/', '#']).next().unwrap_or(&f.path);
                // A compressed disc image counts by the plain image it holds,
                // whose contents romforge can't compare.
                let image = f.game_id.as_ref().and_then(|i| i.image_size);
                let same = rows.iter().find(|r| r.2 == Some(image.unwrap_or(f.size)));
                let verdict = match (same, image) {
                    (Some(_), Some(_)) => "image inside matches DAT ROM size".into(),
                    (None, Some(n)) => format!("image inside is {}; no DAT ROM that size", fmt_bytes(n)),
                    (Some(r), None) if r.4 => {
                        let crc: Option<String> = crc_of
                            .query_row(params![f.sha1, f.size], |r| r.get(0))
                            .optional()?;
                        if crc.is_some() && crc == r.3 {
                            "same size and CRC32 as DAT ROM (no SHA1/MD5 to confirm)".into()
                        } else {
                            "same size as DAT ROM, different contents".into()
                        }
                    }
                    (Some(_), None) => "same size as DAT ROM, different contents".into(),
                    (None, None) => "size differs from every DAT ROM".into(),
                };
                p.facts.push((
                    "Your file".into(),
                    format!("{file} · {} · {verdict}", fmt_bytes(f.size)),
                ));
            }
            let meta: Option<String> = conn
                .query_row(
                    "SELECT value FROM game_meta WHERE system_id = ?1 AND title = ?2 AND field = 'description'",
                    params![sys.id, found.title],
                    |r| r.get(0),
                )
                .optional()?;
            p.description = p.description.or(meta);
            p.image = preview_image(conn, sys, &found.title, cache_dir, thumbnails_base);
        }
        Scraper::Wikidata => {
            p.facts.push(("Wikidata".into(), found.id.clone()));
            if let Some(sys) = d.row.system.as_deref().and_then(systems::by_key) {
                p.image = preview_image(conn, sys, &d.row.title, cache_dir, thumbnails_base);
            }
        }
    }
    Ok(p)
}

/// Box art for a DAT title: the art cache if Enrich saved it, else
/// libretro's thumbnail (saved to `<cache>/preview/<system>/`).
fn preview_image(
    conn: &Connection,
    sys: &systems::System,
    title: &str,
    cache_dir: &Path,
    base: &str,
) -> Option<std::path::PathBuf> {
    use crate::enrich::{ArtKind, libretro_name, thumbnail_url};
    let saved: Option<String> = conn
        .query_row(
            "SELECT path FROM art WHERE system_id = ?1 AND title = ?2 AND kind = 'boxart' AND path IS NOT NULL",
            params![sys.id, title],
            |r| r.get(0),
        )
        .optional()
        .ok()?;
    if let Some(p) = saved.map(std::path::PathBuf::from).filter(|p| p.exists()) {
        return Some(p);
    }
    let dir = cache_dir.join("preview").join(crate::organize::sanitize(sys.key));
    let file = dir.join(format!("{}.png", crate::enrich::cache_stem(title)));
    if file.exists() {
        return Some(file);
    }
    // By libretro's name for the game, as Enrich fetches it.
    let lr_game = crate::enrich::libretro_game(conn, sys, sys.id, title).ok()?;
    let url = thumbnail_url(base, libretro_name(sys)?, ArtKind::Boxart, &lr_game)?;
    let data = crate::fetch::get_image(&url).ok()??;
    crate::enrich::image_ext(&data)?;
    crate::atomic::write(&file, &data).ok()?;
    Some(file)
}

/// What [`choose`] did: the game the row is now, and what Enrich fetched
/// for it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Taken {
    /// Its platform key.
    pub system: String,
    pub title: String,
    pub enrich: crate::enrich::EnrichReport,
}

/// What taking a result did, for the app.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Chosen {
    /// The row's id now (naming a file gives it a new one).
    pub row_id: String,
    pub enrich: crate::enrich::EnrichReport,
}

/// Name or settle a result for a row of `v` (the library's list): a DAT
/// title names its files ("identified by you"); a Wikidata item settles
/// the match (naming an unidentified row by the item's label on `system`
/// first). Does **not** fetch art or metadata — use [`choose`] or Enrich
/// afterwards so Search can move on to the next queue item without waiting.
pub fn apply(
    conn: &mut Connection,
    v: &view::LibraryView,
    row_id: &str,
    found: &Found,
    system: Option<&str>,
    wikidata_endpoint: Option<&str>,
) -> Result<Taken, Error> {
    let d = view::detail_in(conn, v, row_id)?;
    let identified = d.row.status.identified();
    let need_system = || {
        system
            .or(found.system.as_deref())
            .ok_or_else(|| Error::Search("pick a platform first".into()))
    };
    let (key, title) = match found.scraper {
        Scraper::Dat => {
            let key = need_system()?;
            // A translation or hack is named after its base, with the tag.
            let title = found
                .derived
                .as_ref()
                .map_or_else(|| found.title.clone(), view::Derived::title);
            identify_detail(conn, &d, key, &title, found.derived.as_ref())?;
            (key.to_owned(), title)
        }
        Scraper::Wikidata => {
            let (key, title) = if identified {
                (d.row.system.clone().unwrap_or_default(), d.row.title.clone())
            } else {
                let key = need_system()?;
                identify_detail(conn, &d, key, &found.title, None)?;
                (key.to_owned(), found.title.clone())
            };
            let sys = systems::by_key(&key).ok_or_else(|| Error::Search("identify the game first".into()))?;
            let endpoint = wikidata_endpoint.unwrap_or(wikidata::ENDPOINT);
            settle_for(conn, endpoint, sys, &title, &found.id)?;
            (key, title)
        }
    };
    Ok(Taken {
        system: key,
        title,
        enrich: crate::enrich::EnrichReport::default(),
    })
}

/// [`apply`], then fetch for that one game what `opts` asks for (art kinds,
/// Wikidata, `refresh` to overwrite). The app's Search queue uses [`apply`]
/// alone and Enrich in the background so the next item isn't blocked.
pub fn choose(
    conn: &mut Connection,
    v: &view::LibraryView,
    row_id: &str,
    found: &Found,
    system: Option<&str>,
    opts: &crate::enrich::EnrichOptions,
) -> Result<Taken, Error> {
    let mut taken = apply(conn, v, row_id, found, system, opts.wikidata.as_deref())?;
    let mut one = opts.clone();
    one.system = Some(taken.system.clone());
    one.title = Some(taken.title.clone());
    if found.scraper == Scraper::Wikidata {
        one.wikidata = None; // settled and fetched just now
    }
    taken.enrich = crate::enrich::enrich(conn, &one, |_| {})?;
    Ok(taken)
}

/// The row of the game `title` on `system` (a key) in `v`.
pub fn row_in(v: &view::LibraryView, system: &str, title: &str) -> Result<String, Error> {
    v.rows
        .iter()
        .find(|r| {
            r.system.as_deref() == Some(system) && r.title == title && r.status != view::RowStatus::Missing
        })
        .map(|r| r.id.clone())
        .ok_or_else(|| Error::Search(format!("{title} [{system}] is not in the library")))
}

fn fmt_bytes(n: u64) -> String {
    if n >= 1 << 20 && n.is_multiple_of(1 << 20) {
        format!("{} MiB", n >> 20)
    } else if n >= 1 << 20 {
        format!("{:.1} MiB", n as f64 / (1u64 << 20) as f64)
    } else if n >= 1024 {
        format!("{} KiB", n.div_ceil(1024))
    } else {
        format!("{n} bytes")
    }
}
