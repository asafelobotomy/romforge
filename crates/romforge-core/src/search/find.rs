//! Searching a scraper (your DATs, or Wikidata) for a row.

use rusqlite::{Connection, OptionalExtension, params};

use super::{candidates_for, default_text, similarity};
use crate::view;
use crate::{Error, systems, wikidata};

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
    /// DAT results: how strong a suggestion this is (never a match).
    #[serde(default)]
    pub band: Option<crate::suggest::Band>,
    /// DAT results: what agrees and what doesn't.
    #[serde(default)]
    pub evidence: Vec<crate::suggest::Evidence>,
    /// DAT results: taken as a translation or hack of this game (its name
    /// says so).
    #[serde(default)]
    pub derived: Option<view::Derived>,
}

/// Search one scraper for a row of `v` (the library's list). `text`: what
/// the owner typed (the row's title or file name when empty); `system`:
/// limit to this platform key.
#[allow(clippy::too_many_arguments)]
pub fn find(
    conn: &Connection,
    v: &view::LibraryView,
    row_id: &str,
    scraper: Scraper,
    text: Option<&str>,
    system: Option<&str>,
    api: &str,
    databases: Option<&std::path::Path>,
) -> Result<Vec<Found>, Error> {
    let d = view::detail_in(conn, v, row_id)?;
    match scraper {
        Scraper::Dat => {
            let mut year = conn.prepare_cached(
                "SELECT value FROM game_meta m JOIN systems s ON s.system_id = m.system_id
                 WHERE s.key = ?1 AND m.title = ?2 AND m.field = 'releaseyear'",
            )?;
            crate::suggest::for_detail(conn, databases, &d, text, system, 30)?
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
                        description: Some(match &t.derived {
                            Some(d) => format!("{} · {}", t.band.label(), d.label()),
                            None => t.band.label().to_owned(),
                        }),
                        band: Some(t.band),
                        evidence: t.evidence,
                        derived: t.derived,
                    })
                })
                .collect()
        }
        Scraper::Wikidata => {
            let text = text
                .filter(|t| !t.trim().is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| strip_tags(&default_text(&d)));
            let system = system.map(str::to_owned).or(d.row.system.clone());
            // When the platform is known, only video games on that platform
            // (same P400 + P31 filter Enrich uses). Else any entity.
            let platforms = system
                .as_deref()
                .and_then(systems::by_key)
                .map(|s| s.wikidata)
                .filter(|p| !p.is_empty());
            let mut out = match platforms {
                Some(p) => search_on_platforms(api, &text, p, system.as_deref())?,
                None => {
                    let url = format!(
                        "{api}?action=wbsearchentities&format=json&language=en&uselang=en&type=item&limit=20&search={}",
                        crate::fetch::encode_segment(&text)
                    );
                    parse_entity_search(&crate::fetch::get_small(&url)?, &text, system.as_deref())?
                }
            };
            // The candidates Enrich found equally good come too.
            for c in candidates_for(conn, &d)? {
                if !out.iter().any(|f| f.id == c.qid) {
                    out.push(Found {
                        scraper,
                        score: similarity(&text, &c.label),
                        year: c.years.first().copied(),
                        id: c.qid,
                        title: c.label,
                        system: system.clone(),
                        description: Some("candidate found by Enrich".into()),
                        band: None,
                        evidence: Vec::new(),
                        derived: None,
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

/// MediaWiki search + entity labels, limited to video games on `platforms`.
fn search_on_platforms(
    api: &str,
    text: &str,
    platforms: &[&str],
    system: Option<&str>,
) -> Result<Vec<Found>, Error> {
    let mut ids: Vec<String> = Vec::new();
    for platform in platforms {
        let sr = format!(
            "{text} haswbstatement:P400={platform} haswbstatement:P31={}|P31={}",
            wikidata::GAME,
            wikidata::EDITION
        );
        let url = format!(
            "{api}?action=query&list=search&format=json&srlimit=20&srsearch={}",
            crate::fetch::encode_segment(&sr)
        );
        for id in parse_page_ids(&crate::fetch::get_small(&url)?)? {
            if !ids.iter().any(|x| x == &id) {
                ids.push(id);
            }
        }
    }
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let url = format!(
        "{api}?action=wbgetentities&format=json&props=labels|descriptions&languages=en|mul&ids={}",
        ids.join("|")
    );
    parse_entities(&crate::fetch::get_small(&url)?, text, system, &ids)
}

/// A DAT name as Wikidata would label it: [`view::search_title`] (no tags,
/// "Legend of Zelda, The" as "The Legend of Zelda"), and No-Intro's " - "
/// back to the colon it stands for.
pub(super) fn strip_tags(title: &str) -> String {
    view::search_title(title).replacen(" - ", ": ", 1)
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
            Some(found(id, title, description, text, system))
        })
        .collect())
}

/// Item ids from MediaWiki `list=search` (`title` is the Q id).
pub(super) fn parse_page_ids(body: &[u8]) -> Result<Vec<String>, Error> {
    let v: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| Error::Http(format!("Wikidata search answer: {e}")))?;
    let hits = v["query"]["search"]
        .as_array()
        .ok_or_else(|| Error::Http("Wikidata search answer has no results".into()))?;
    Ok(hits
        .iter()
        .filter_map(|h| {
            let id = h["title"].as_str()?;
            wikidata::is_qid(id).then(|| id.to_owned())
        })
        .collect())
}

/// Labels and descriptions from `wbgetentities`, in `order`.
pub(super) fn parse_entities(
    body: &[u8],
    text: &str,
    system: Option<&str>,
    order: &[String],
) -> Result<Vec<Found>, Error> {
    let v: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| Error::Http(format!("Wikidata entities answer: {e}")))?;
    let ents = v["entities"]
        .as_object()
        .ok_or_else(|| Error::Http("Wikidata entities answer has no entities".into()))?;
    Ok(order
        .iter()
        .filter_map(|id| {
            let e = ents.get(id)?;
            let title = lang_value(e, "labels")?;
            let description = lang_value(e, "descriptions");
            Some(found(id.clone(), title, description, text, system))
        })
        .collect())
}

fn lang_value(entity: &serde_json::Value, field: &str) -> Option<String> {
    let map = entity.get(field)?;
    map["en"]["value"]
        .as_str()
        .or_else(|| map["mul"]["value"].as_str())
        .map(str::to_owned)
}

fn found(id: String, title: String, description: Option<String>, text: &str, system: Option<&str>) -> Found {
    let year = description.as_deref().and_then(first_year);
    Found {
        scraper: Scraper::Wikidata,
        score: similarity(text, &title),
        id,
        title,
        system: system.map(str::to_owned),
        year,
        description,
        band: None,
        evidence: Vec::new(),
        derived: None,
    }
}

/// The first plausible release year in a text ("1991 video game").
pub(super) fn first_year(s: &str) -> Option<i32> {
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
