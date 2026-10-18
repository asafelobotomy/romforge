//! Milestone 8: the Search queue. Games and files that need the owner,
//! one by one: identify a file no hash matched (DAT title suggestions, or a
//! title of your own), and settle a Wikidata match romforge couldn't make.
//!
//! A title picked here is "identified by you" (house rule 8): stored by the
//! file's SHA1 and size, shown apart from hash matches, never counted as
//! verified.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::view::{self, RowStatus};
use crate::{Error, systems, wikidata};

/// What a queue item needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Need {
    /// No hash identified it: pick a title.
    Identify,
    /// Identified, but Wikidata couldn't be matched on its own.
    Wikidata,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct QueueItem {
    pub row_id: String,
    pub title: String,
    pub system: Option<String>,
    pub status: RowStatus,
    pub need: Need,
    /// For [`Need::Wikidata`]: "ambiguous" or "none".
    pub wikidata: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TitleSuggestion {
    pub system: String,
    pub title: String,
    /// 0..1: how much of the two titles' words overlap.
    pub score: f32,
}

/// Rows that need the owner, in list order. `only`: limit to these row ids.
pub fn queue(conn: &Connection, only: Option<&[String]>) -> Result<Vec<QueueItem>, Error> {
    let v = view::library(conn)?;
    let mut wd: BTreeMap<(String, String), String> = BTreeMap::new();
    for r in conn
        .prepare(
            "SELECT s.key, w.title, w.status FROM wikidata_match w
             JOIN systems s ON s.system_id = w.system_id WHERE w.status IN ('ambiguous', 'none')",
        )?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
    {
        let (s, t, st) = r?;
        wd.insert((s, t), st);
    }
    let mut out = Vec::new();
    for r in v.rows {
        if only.is_some_and(|ids| !ids.contains(&r.id)) {
            continue;
        }
        let identified = matches!(r.status, RowStatus::Verified | RowStatus::ByYou);
        if !identified && r.status != RowStatus::Duplicate {
            out.push(QueueItem {
                row_id: r.id.clone(),
                title: r.title.clone(),
                system: r.system.clone(),
                status: r.status,
                need: Need::Identify,
                wikidata: None,
            });
        } else if let Some(system) = &r.system
            && let Some(st) = wd.get(&(system.clone(), r.title.clone()))
        {
            out.push(QueueItem {
                row_id: r.id.clone(),
                title: r.title.clone(),
                system: r.system.clone(),
                status: r.status,
                need: Need::Wikidata,
                wikidata: Some(st.clone()),
            });
        }
    }
    Ok(out)
}

/// How alike two titles are, 0 to 1 ([`crate::titles::score`]): numbers
/// and edition words must agree; a missing subtitle scores 0.9.
pub fn similarity(a: &str, b: &str) -> f32 {
    crate::titles::score(a, b)
}

/// DAT titles that look like `text` (a row's title, or what the owner
/// typed), best first. Only systems the row could belong to: its own, or
/// those using its file extension; all systems when there is no hint.
pub fn title_suggestions(
    conn: &Connection,
    row_id: &str,
    text: Option<&str>,
    limit: usize,
) -> Result<Vec<TitleSuggestion>, Error> {
    let d = view::detail(conn, row_id)?;
    dat_titles(conn, &d, text, None, limit)
}

/// What to search for when the owner typed nothing: the row's title, or
/// its file name without the extension and the archive part.
pub fn default_text(d: &view::GameDetail) -> String {
    let t = d.row.title.rsplit('#').next().unwrap_or(&d.row.title);
    if d.row.system.is_some() {
        return t.to_owned();
    }
    Path::new(t)
        .file_stem()
        .map_or(t.to_owned(), |s| s.to_string_lossy().into_owned())
}

/// [`title_suggestions`] for a row, on `system` (a key) when given.
fn dat_titles(
    conn: &Connection,
    d: &view::GameDetail,
    text: Option<&str>,
    system: Option<&str>,
    limit: usize,
) -> Result<Vec<TitleSuggestion>, Error> {
    let text = text
        .filter(|t| !t.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| default_text(d));
    let mut system_ids: Vec<i64> = match system.or(d.row.system.as_deref()) {
        Some(key) => systems::by_key(key).map(|s| vec![s.id]).unwrap_or_default(),
        None => Vec::new(),
    };
    if system_ids.is_empty() {
        let ext = d.files.first().and_then(|f| {
            Path::new(f.path.rsplit('#').next().unwrap_or(&f.path))
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
        });
        if let Some(ext) = ext {
            system_ids = systems::SYSTEMS
                .iter()
                .filter(|s| s.extensions.contains(&ext.as_str()))
                .map(|s| s.id)
                .collect();
        }
    }
    let mut stmt = conn.prepare(
        "SELECT DISTINCT d.system_id, g.name FROM games g JOIN dats d ON d.dat_id = g.dat_id
         WHERE d.system_id IS NOT NULL AND (?1 = 0 OR d.system_id IN (SELECT value FROM json_each(?2)))",
    )?;
    let ids_json = serde_json::to_string(&system_ids).unwrap_or_else(|_| "[]".into());
    // name -> systems with it; the index scores each name once.
    let mut systems_of: std::collections::HashMap<String, Vec<i64>> = Default::default();
    for r in stmt.query_map(params![system_ids.len() as i64, ids_json], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })? {
        let (sid, name) = r?;
        systems_of.entry(name).or_default().push(sid);
    }
    let index = crate::titles::TitleIndex::new(systems_of.keys().cloned());
    let mut scored: Vec<TitleSuggestion> = Vec::new();
    for (title, score) in index.find(&text, limit) {
        for sid in &systems_of[&title] {
            scored.push(TitleSuggestion {
                system: systems::by_id(*sid).map_or(String::new(), |s| s.key.to_owned()),
                title: title.clone(),
                score,
            });
        }
    }
    scored.truncate(limit);
    Ok(scored)
}

/// Name every file of a row: `title` on `system` (a key). Re-identifies the
/// library so the files show as "identified by you".
pub fn identify(conn: &mut Connection, row_id: &str, system: &str, title: &str) -> Result<(), Error> {
    let title = title.trim();
    if title.is_empty() {
        return Err(Error::Search("a title is needed".into()));
    }
    let sys = systems::by_key(system).ok_or_else(|| Error::UnknownSystem(system.to_owned()))?;
    let d = view::detail(conn, row_id)?;
    if matches!(d.row.status, RowStatus::Verified) {
        return Err(Error::Search(format!(
            "{} is already identified by its hashes",
            d.row.title
        )));
    }
    let tx = conn.transaction()?;
    for f in &d.files {
        tx.execute(
            "INSERT INTO manual_ids (sha1, size, system_id, title, created_at)
             VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
             ON CONFLICT(sha1, size) DO UPDATE SET system_id = excluded.system_id, title = excluded.title",
            params![f.sha1, f.size, sys.id, title],
        )?;
    }
    crate::identify::identify_all(&tx)?;
    tx.commit()?;
    Ok(())
}

/// Undo [`identify`] for a row's files.
pub fn forget(conn: &mut Connection, row_id: &str) -> Result<(), Error> {
    let d = view::detail(conn, row_id)?;
    let tx = conn.transaction()?;
    for f in &d.files {
        tx.execute(
            "DELETE FROM manual_ids WHERE sha1 = ?1 AND size = ?2",
            params![f.sha1, f.size],
        )?;
    }
    crate::identify::identify_all(&tx)?;
    tx.commit()?;
    Ok(())
}

/// The Wikidata candidates kept for an ambiguous row.
pub fn wikidata_candidates(conn: &Connection, row_id: &str) -> Result<Vec<wikidata::Candidate>, Error> {
    let d = view::detail(conn, row_id)?;
    let Some(sys) = d.row.system.as_deref().and_then(systems::by_key) else {
        return Ok(Vec::new());
    };
    let json: Option<String> = conn
        .query_row(
            "SELECT candidates FROM wikidata_match WHERE system_id = ?1 AND title = ?2",
            params![sys.id, d.row.title],
            |r| r.get(0),
        )
        .optional()?;
    Ok(json
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default())
}

/// Settle a row's Wikidata item by hand (`qid`, e.g. "Q12345"), then fetch
/// its details and series like any match.
pub fn settle_wikidata(conn: &mut Connection, endpoint: &str, row_id: &str, qid: &str) -> Result<(), Error> {
    let qid = qid.trim();
    if !(qid.starts_with('Q') && qid.len() > 1 && qid[1..].chars().all(|c| c.is_ascii_digit())) {
        return Err(Error::Search(format!(
            "{qid:?} is not a Wikidata item id (like Q12345)"
        )));
    }
    let d = view::detail(conn, row_id)?;
    let sys = d
        .row
        .system
        .as_deref()
        .and_then(systems::by_key)
        .ok_or_else(|| Error::Search("identify the game first".into()))?;
    conn.execute(
        "INSERT INTO wikidata_match (system_id, title, qid, status, candidates, checked_at)
         VALUES (?1, ?2, ?3, 'by_you', '[]', strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
         ON CONFLICT(system_id, title) DO UPDATE SET qid = excluded.qid, status = 'by_you',
           checked_at = excluded.checked_at",
        params![sys.id, d.row.title, qid],
    )?;
    let mut one = BTreeMap::new();
    one.insert((sys.id, d.row.title.clone()), Vec::new());
    let r = wikidata::enrich(conn, endpoint, &one, false, |_| {})?;
    if let Some(e) = r.errors.first() {
        return Err(Error::Search(format!(
            "saved, but fetching its details failed: {e}"
        )));
    }
    Ok(())
}

/// Where a Search result comes from (the "Scraper" of the dialog).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scraper {
    /// Game names in your DATs (offline).
    Dat,
    /// Wikidata items, by label (free, no account).
    Wikidata,
}

/// Wikidata's entity search API (not the SPARQL query service).
pub const WIKIDATA_API: &str = "https://www.wikidata.org/w/api.php";

/// One row of the results table.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Found {
    pub scraper: Scraper,
    /// DAT: the game name; Wikidata: the item id (Q…).
    pub id: String,
    pub title: String,
    /// System key.
    pub system: Option<String>,
    pub year: Option<i32>,
    /// 0..1: how much of the searched words the title shares.
    pub score: f32,
    /// Wikidata's one-line description ("1991 video game").
    pub description: Option<String>,
}

/// Search one scraper for a row. `text`: what the owner typed (the row's
/// title or file name when empty); `system`: limit to this platform key.
pub fn find(
    conn: &Connection,
    row_id: &str,
    scraper: Scraper,
    text: Option<&str>,
    system: Option<&str>,
    api: &str,
) -> Result<Vec<Found>, Error> {
    let d = view::detail(conn, row_id)?;
    match scraper {
        Scraper::Dat => {
            let mut year = conn.prepare_cached(
                "SELECT value FROM game_meta m JOIN systems s ON s.system_id = m.system_id
                 WHERE s.key = ?1 AND m.title = ?2 AND m.field = 'releaseyear'",
            )?;
            dat_titles(conn, &d, text, system, 30)?
                .into_iter()
                .map(|t| {
                    let y: Option<String> = year
                        .query_row(params![t.system, t.title], |r| r.get(0))
                        .optional()?;
                    Ok(Found {
                        scraper,
                        id: t.title.clone(),
                        title: t.title,
                        system: Some(t.system),
                        year: y.and_then(|y| y.parse().ok()),
                        score: t.score,
                        description: None,
                    })
                })
                .collect()
        }
        Scraper::Wikidata => {
            let text = text
                .filter(|t| !t.trim().is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| strip_tags(&default_text(&d)));
            let url = format!(
                "{api}?action=wbsearchentities&format=json&language=en&uselang=en&type=item&limit=20&search={}",
                crate::fetch::encode_segment(&text)
            );
            let body = crate::fetch::get(&url)?;
            let system = system.map(str::to_owned).or(d.row.system.clone());
            let mut out = parse_entity_search(&body, &text, system.as_deref())?;
            // The candidates Enrich found equally good come too.
            for c in wikidata_candidates(conn, row_id)? {
                if !out.iter().any(|f| f.id == c.qid) {
                    out.push(Found {
                        scraper,
                        score: similarity(&text, &c.label),
                        year: c.years.first().copied(),
                        id: c.qid,
                        title: c.label,
                        system: system.clone(),
                        description: Some("candidate found by Enrich".into()),
                    });
                }
            }
            // By how well the label fits; among equals, video games first.
            out.sort_by(|a, b| {
                let other = |f: &Found| !f.description.as_deref().is_some_and(|d| d.contains("game"));
                b.score.total_cmp(&a.score).then(other(a).cmp(&other(b)))
            });
            Ok(out)
        }
    }
}

/// A DAT name as Wikidata would label it: no tags ("Super Mario World
/// (USA) (Rev 1)" -> "Super Mario World"), and No-Intro's " - " back to the
/// colon it stands for.
fn strip_tags(title: &str) -> String {
    let cut = [title.find(" ("), title.find(" [")].into_iter().flatten().min();
    let t = &title[..cut.unwrap_or(title.len())];
    // "Legend of Zelda, The - …" -> "The Legend of Zelda - …"
    let t = match t.split_once(", The") {
        Some((a, b)) => format!("The {a}{b}"),
        None => t.to_owned(),
    };
    t.replacen(" - ", ": ", 1)
}

/// The answer of `wbsearchentities`.
pub fn parse_entity_search(body: &[u8], text: &str, system: Option<&str>) -> Result<Vec<Found>, Error> {
    let v: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| Error::Http(format!("Wikidata search answer: {e}")))?;
    let hits = v["search"]
        .as_array()
        .ok_or_else(|| Error::Http("Wikidata search answer has no results".into()))?;
    Ok(hits
        .iter()
        .filter_map(|h| {
            let id = h["id"].as_str()?.to_owned();
            let title = h["label"]
                .as_str()
                .or(h["display"]["label"]["value"].as_str())?
                .to_owned();
            let description = h["description"].as_str().map(str::to_owned);
            let year = description.as_deref().and_then(first_year);
            Some(Found {
                scraper: Scraper::Wikidata,
                score: similarity(text, &title),
                id,
                title,
                system: system.map(str::to_owned),
                year,
                description,
            })
        })
        .collect())
}

/// The first plausible release year in a text ("1991 video game").
fn first_year(s: &str) -> Option<i32> {
    s.as_bytes()
        .windows(4)
        .enumerate()
        .filter(|(i, w)| {
            w.iter().all(u8::is_ascii_digit)
                && !s.as_bytes().get(i + 4).is_some_and(u8::is_ascii_digit)
                && (*i == 0 || !s.as_bytes()[i - 1].is_ascii_digit())
        })
        .filter_map(|(_, w)| std::str::from_utf8(w).ok()?.parse().ok())
        .find(|y| (1970..=2100).contains(y))
}

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
                "SELECT d.name, r.name, r.size FROM games g JOIN dats d ON d.dat_id = g.dat_id
                 JOIN roms r ON r.game_id = g.game_id
                 WHERE d.system_id = ?1 AND g.name = ?2 ORDER BY d.dat_id, r.rom_id",
            )?;
            let rows: Vec<(String, String, Option<u64>)> = stmt
                .query_map(params![sys.id, found.title], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })?
                .collect::<Result<_, _>>()?;
            let dats: BTreeSet<&str> = rows.iter().map(|r| r.0.as_str()).collect();
            if !dats.is_empty() {
                p.facts
                    .push(("DAT".into(), dats.into_iter().collect::<Vec<_>>().join(", ")));
            }
            let mut seen = BTreeSet::new();
            for (_, name, size) in &rows {
                if seen.insert(name) {
                    let size = size.map_or("?".into(), |s| format!("{s} bytes"));
                    p.facts.push(("ROM".into(), format!("{name} · {size}")));
                }
            }
            for f in &d.files {
                let file = f.path.rsplit(['/', '#']).next().unwrap_or(&f.path);
                let same = rows.iter().find(|r| r.2 == Some(f.size));
                let verdict = match same {
                    Some(r) => format!("same size as {}, different contents", r.1),
                    None => "size differs from every ROM".into(),
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
    let file = dir.join(format!("{}.png", crate::organize::sanitize(title)));
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
    let identified = matches!(d.row.status, RowStatus::Verified | RowStatus::ByYou);
    let need_system = || {
        system
            .or(found.system.as_deref())
            .ok_or_else(|| Error::Search("pick a platform first".into()))
    };
    let (key, title) = match found.scraper {
        Scraper::Dat => {
            let key = need_system()?;
            identify(conn, row_id, key, &found.title)?;
            (key.to_owned(), found.title.clone())
        }
        Scraper::Wikidata => {
            let (key, title) = if identified {
                (d.row.system.clone().unwrap_or_default(), d.row.title.clone())
            } else {
                let key = need_system()?;
                identify(conn, row_id, key, &found.title)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similarity_ranks_by_shared_words() {
        let q = "Dragon Quest III (English v2.0)[Addendum]";
        let a = similarity(q, "Dragon Quest III - Soshite Densetsu e... (Japan)");
        let b = similarity(q, "Dragon Quest I & II (Japan)");
        let c = similarity(q, "Super Mario World (USA)");
        assert_eq!(a, 0.9, "same game, subtitle missing");
        assert_eq!(b, 0.0, "numerals differ");
        assert_eq!(c, 0.0);
    }

    #[test]
    fn wikidata_search_results() {
        let body = br#"{"search":[
            {"id":"Q853143","label":"Super Mario World","description":"1990 video game"},
            {"id":"Q1","label":"Super Mario World 2: Yoshi's Island","description":"video game from 1995"},
            {"id":"Q2","label":"Nameless"}]}"#;
        let r = parse_entity_search(body, "Super Mario World", Some("SNES")).unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(
            (r[0].id.as_str(), r[0].year, r[0].score),
            ("Q853143", Some(1990), 1.0)
        );
        assert_eq!(r[1].year, Some(1995));
        assert_eq!(r[2].year, None);
        assert_eq!(r[0].system.as_deref(), Some("SNES"));
    }

    #[test]
    fn tags_and_articles_go_for_wikidata() {
        assert_eq!(strip_tags("Super Mario World (USA) (Rev 1)"), "Super Mario World");
        assert_eq!(
            strip_tags("Legend of Zelda, The - A Link to the Past (USA)"),
            "The Legend of Zelda: A Link to the Past"
        );
        assert_eq!(
            strip_tags("Castlevania - Dracula X (USA)"),
            "Castlevania: Dracula X"
        );
        assert_eq!(strip_tags("Tetris"), "Tetris");
        assert_eq!(first_year("ca. 12345 and 1991"), Some(1991));
    }
}
