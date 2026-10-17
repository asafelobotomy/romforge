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

fn words(s: &str) -> BTreeSet<String> {
    wikidata::normalize(s)
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// How alike two titles are: shared words over all words (Jaccard), so
/// "dragon quest iii english" vs "Dragon Quest III (Japan)" scores well.
pub fn similarity(a: &str, b: &str) -> f32 {
    let (x, y) = (words(a), words(b));
    if x.is_empty() || y.is_empty() {
        return 0.0;
    }
    let shared = x.intersection(&y).count() as f32;
    shared / x.union(&y).count() as f32
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
    let text = text.map(str::to_owned).unwrap_or_else(|| {
        // A file name: drop the extension and the archive part.
        let t = d.row.title.rsplit('#').next().unwrap_or(&d.row.title);
        Path::new(t)
            .file_stem()
            .map_or(t.to_owned(), |s| s.to_string_lossy().into_owned())
    });
    let mut system_ids: Vec<i64> = match &d.row.system {
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
    let mut scored: Vec<TitleSuggestion> = Vec::new();
    for r in stmt.query_map(params![system_ids.len() as i64, ids_json], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })? {
        let (sid, name) = r?;
        let score = similarity(&text, &name);
        if score > 0.0 {
            scored.push(TitleSuggestion {
                system: systems::by_id(sid).map_or(String::new(), |s| s.key.to_owned()),
                title: name,
                score,
            });
        }
    }
    scored.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similarity_ranks_by_shared_words() {
        let q = "Dragon Quest III (English v2.0)[Addendum]";
        let a = similarity(q, "Dragon Quest III - Soshite Densetsu e... (Japan)");
        let b = similarity(q, "Dragon Quest I & II (Japan)");
        let c = similarity(q, "Super Mario World (USA)");
        assert!(a > b && b > c, "{a} {b} {c}");
        assert_eq!(c, 0.0);
    }
}
