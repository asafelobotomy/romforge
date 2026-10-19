//! Searching a scraper (your DATs, or Wikidata) for a row.

use rusqlite::{Connection, OptionalExtension, params};

use super::{default_text, similarity, wikidata_candidates};
use crate::Error;
use crate::view;

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
            crate::suggest::for_detail(conn, &d, text, system, 30)?
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

/// A DAT name as Wikidata would label it: no tags ("Super Mario World
/// (USA) (Rev 1)" -> "Super Mario World"), and No-Intro's " - " back to the
/// colon it stands for.
pub(super) fn strip_tags(title: &str) -> String {
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
                band: None,
                evidence: Vec::new(),
                derived: None,
            })
        })
        .collect())
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
