//! The preview of a result, and taking one for a row.

use std::collections::BTreeSet;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use super::{Found, Scraper, identify, settle_wikidata};
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

/// The preview of a result for a row. Never fails over a missing picture.
pub fn preview(
    conn: &Connection,
    row_id: &str,
    found: &Found,
    cache_dir: &Path,
    thumbnails_base: &str,
) -> Result<Preview, Error> {
    let d = view::detail(conn, row_id)?;
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
                    let size = size.map_or("?".into(), |s| format!("{s} bytes"));
                    p.facts.push(("ROM".into(), format!("{name} · {size}")));
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
                    (Some(r), Some(_)) => format!("holds an image the size of {}", r.1),
                    (None, Some(n)) => format!("holds a {n}-byte image, a size no ROM has"),
                    (Some(r), None) if r.4 => {
                        let crc: Option<String> = crc_of
                            .query_row(params![f.sha1, f.size], |r| r.get(0))
                            .optional()?;
                        if crc.is_some() && crc == r.3 {
                            format!("same size and CRC32 as {} (no SHA1 or MD5 to confirm)", r.1)
                        } else {
                            format!("same size as {}, different contents", r.1)
                        }
                    }
                    (Some(r), None) => format!("same size as {}, different contents", r.1),
                    (None, None) => "size differs from every ROM".into(),
                };
                p.facts.push((
                    "Your file".into(),
                    format!("{file} · {} bytes · {verdict}", f.size),
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
    let url = thumbnail_url(base, libretro_name(sys)?, ArtKind::Boxart, title)?;
    let data = crate::fetch::request(&crate::fetch::Request::get(&url)).ok()??;
    crate::enrich::image_ext(&data)?;
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(&file, data).ok()?;
    Some(file)
}

/// What [`choose`] did.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Chosen {
    /// The row's id now (naming a file gives it a new one).
    pub row_id: String,
    pub enrich: crate::enrich::EnrichReport,
}

/// Take a result for a row: a DAT title names its files ("identified by
/// you"); a Wikidata item settles the match (naming an unidentified row by
/// the item's label on `system` first). Then fetches for that one game what
/// `opts` asks for (art kinds, Wikidata, `refresh` to overwrite).
pub fn choose(
    conn: &mut Connection,
    row_id: &str,
    found: &Found,
    system: Option<&str>,
    opts: &crate::enrich::EnrichOptions,
) -> Result<Chosen, Error> {
    let d = view::detail(conn, row_id)?;
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
            identify(conn, row_id, key, &title, found.derived.as_ref())?;
            (key.to_owned(), title)
        }
        Scraper::Wikidata => {
            let (key, title) = if identified {
                (d.row.system.clone().unwrap_or_default(), d.row.title.clone())
            } else {
                let key = need_system()?;
                identify(conn, row_id, key, &found.title, None)?;
                (key.to_owned(), found.title.clone())
            };
            let row = row_for(conn, &key, &title)?;
            let endpoint = opts.wikidata.as_deref().unwrap_or(wikidata::ENDPOINT);
            settle_wikidata(conn, endpoint, &row, &found.id)?;
            (key, title)
        }
    };
    let row_id = row_for(conn, &key, &title)?;
    let mut one = opts.clone();
    one.system = Some(key);
    one.title = Some(title);
    if found.scraper == Scraper::Wikidata {
        one.wikidata = None; // settled and fetched just now
    }
    let enrich = crate::enrich::enrich(conn, &one, |_| {})?;
    Ok(Chosen { row_id, enrich })
}

/// The library row of a game.
fn row_for(conn: &Connection, system: &str, title: &str) -> Result<String, Error> {
    view::library(conn)?
        .rows
        .into_iter()
        .find(|r| r.system.as_deref() == Some(system) && r.title == title)
        .map(|r| r.id)
        .ok_or_else(|| Error::Search(format!("{title} [{system}] is not in the library")))
}
