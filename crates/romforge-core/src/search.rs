//! Milestone 8: the Search queue. Games and files that need the owner,
//! one by one: identify a file no hash matched (DAT title suggestions, or a
//! title of your own), and settle a Wikidata match romforge couldn't make.
//!
//! A title picked here is "identified by you" (house rule 8): stored by the
//! file's SHA1 and size, shown apart from hash matches, never counted as
//! verified.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::view::{self, RowStatus};
use crate::{Error, systems, wikidata};

mod find;
mod preview;
#[cfg(test)]
mod tests;

pub use find::{Found, Scraper, WIKIDATA_API, find, parse_entity_search};
pub use preview::{Chosen, Preview, Taken, choose, preview, row_in};

/// What a queue item needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Need {
    /// No hash identified it: pick a title.
    Identify,
    /// Identified, but Wikidata couldn't be matched on its own.
    Wikidata,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct QueueItem {
    pub row_id: String,
    pub title: String,
    pub system: Option<String>,
    pub status: RowStatus,
    pub need: Need,
    /// For [`Need::Wikidata`]: "ambiguous" or "none".
    pub wikidata: Option<String>,
    /// For [`Need::Identify`]: the strongest suggestion, with its evidence
    /// (never a match).
    pub suggestion: Option<crate::suggest::Suggestion>,
}

/// Rows of `v` (the library's list, [`view::listing`]) that need the owner,
/// in list order. `only`: limit to these row ids. Patch files and missing
/// originals are listed only, never queued.
pub fn queue(
    conn: &Connection,
    v: &view::LibraryView,
    only: Option<&[String]>,
) -> Result<Vec<QueueItem>, Error> {
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
    let mut suggester = crate::suggest::Suggester::default();
    for r in &v.rows {
        if only.is_some_and(|ids| !ids.contains(&r.id))
            || matches!(r.status, RowStatus::Patch | RowStatus::Missing)
        {
            continue;
        }
        let identified = r.status.identified();
        if !identified && r.status != RowStatus::Duplicate {
            let d = view::detail_in(conn, v, &r.id)?;
            let suggestion = suggester.suggest(conn, &d, None, None, 1)?.into_iter().next();
            out.push(QueueItem {
                row_id: r.id.clone(),
                title: r.title.clone(),
                system: r.system.clone(),
                status: r.status,
                need: Need::Identify,
                wikidata: None,
                suggestion,
            });
        } else if r.status != RowStatus::Derivative
            // A translation or hack shows its base game's Wikidata facts.
            && let Some(system) = &r.system
            && let Some(st) = wd.get(&(system.clone(), r.title.clone()))
        {
            out.push(QueueItem {
                row_id: r.id.clone(),
                title: r.title.clone(),
                system: r.system.clone(),
                status: r.status,
                need: Need::Wikidata,
                wikidata: Some(st.clone()),
                suggestion: None,
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

/// A row of the list that has no files of its own to name: a patch file,
/// or the original of versions you have.
fn has_own_files(d: &view::GameDetail) -> Result<(), Error> {
    match d.row.status {
        RowStatus::Patch => Err(Error::Search(format!(
            "{} is a patch file: apply it to its game instead",
            d.row.title
        ))),
        RowStatus::Missing => Err(Error::Search(format!(
            "you don't have {}: name the files of a version you have",
            d.row.title
        ))),
        _ => Ok(()),
    }
}

/// Name every file of a row of `v` (the library's list): `title` on
/// `system` (a key), as a translation or hack of a DAT game when `derived`
/// says so. Re-identifies the library so the files show as "identified by
/// you" (or "translation of X").
pub fn identify(
    conn: &mut Connection,
    v: &view::LibraryView,
    row_id: &str,
    system: &str,
    title: &str,
    derived: Option<&view::Derived>,
) -> Result<(), Error> {
    let d = view::detail_in(conn, v, row_id)?;
    identify_detail(conn, &d, system, title, derived)
}

/// [`identify`] for a row already looked up.
fn identify_detail(
    conn: &mut Connection,
    d: &view::GameDetail,
    system: &str,
    title: &str,
    derived: Option<&view::Derived>,
) -> Result<(), Error> {
    let title = title.trim();
    if title.is_empty() {
        return Err(Error::Search("a title is needed".into()));
    }
    let sys = systems::by_key(system).ok_or_else(|| Error::UnknownSystem(system.to_owned()))?;
    has_own_files(d)?;
    // Files matched by hash in their folder's scope stay matched: a pick
    // would have no effect, so say so (an incomplete game's files too).
    if matches!(d.row.status, RowStatus::Verified)
        || (!d.files.is_empty()
            && d.files
                .iter()
                .all(|f| matches!(f.status.as_str(), "matched" | "duplicate")))
    {
        return Err(Error::Search(format!(
            "{} is already identified by its hashes",
            d.row.title
        )));
    }
    let tx = conn.transaction()?;
    for f in &d.files {
        tx.execute(
            concat!(
                "INSERT INTO manual_ids (sha1, size, system_id, title, created_at, base, relation, tag)
             VALUES (?1, ?2, ?3, ?4, ",
                crate::db::now!(),
                ", ?5, ?6, ?7)
             ON CONFLICT(sha1, size) DO UPDATE SET system_id = excluded.system_id, title = excluded.title,
               base = excluded.base, relation = excluded.relation, tag = excluded.tag, proof = NULL"
            ),
            params![
                f.sha1,
                f.size,
                sys.id,
                title,
                derived.map(|d| &d.base),
                derived.map(|d| d.relation.as_str()),
                derived.and_then(|d| d.tag.as_deref()),
            ],
        )?;
    }
    crate::identify::identify(&tx, crate::identify::Relook::Nothing)?;
    tx.commit()?;
    Ok(())
}

/// Undo [`identify`] for a row's files.
pub fn forget(conn: &mut Connection, v: &view::LibraryView, row_id: &str) -> Result<(), Error> {
    let d = view::detail_in(conn, v, row_id)?;
    has_own_files(&d)?;
    let tx = conn.transaction()?;
    for f in &d.files {
        // A name romforge gave from a patch's proof isn't given again.
        tx.execute(
            "INSERT OR IGNORE INTO proofs_declined (sha1, size)
             SELECT sha1, size FROM manual_ids WHERE sha1 = ?1 AND size = ?2 AND proof IS NOT NULL",
            params![f.sha1, f.size],
        )?;
        tx.execute(
            "DELETE FROM manual_ids WHERE sha1 = ?1 AND size = ?2",
            params![f.sha1, f.size],
        )?;
    }
    crate::identify::identify(&tx, crate::identify::Relook::Nothing)?;
    tx.commit()?;
    Ok(())
}

/// The Wikidata candidates kept for an ambiguous row.
pub fn wikidata_candidates(
    conn: &Connection,
    v: &view::LibraryView,
    row_id: &str,
) -> Result<Vec<wikidata::Candidate>, Error> {
    candidates_for(conn, &view::detail_in(conn, v, row_id)?)
}

/// [`wikidata_candidates`] for a row already looked up.
fn candidates_for(conn: &Connection, d: &view::GameDetail) -> Result<Vec<wikidata::Candidate>, Error> {
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
pub fn settle_wikidata(
    conn: &mut Connection,
    v: &view::LibraryView,
    endpoint: &str,
    row_id: &str,
    qid: &str,
) -> Result<(), Error> {
    let d = view::detail_in(conn, v, row_id)?;
    if d.row.status == RowStatus::Patch {
        return Err(Error::Search(format!(
            "{} is a patch file: settle its game instead",
            d.row.title
        )));
    }
    let sys = d
        .row
        .system
        .as_deref()
        .and_then(systems::by_key)
        .ok_or_else(|| Error::Search("identify the game first".into()))?;
    settle_for(conn, endpoint, sys, &d.row.title, qid)
}

/// [`settle_wikidata`] for the game `title` on `sys`.
fn settle_for(
    conn: &mut Connection,
    endpoint: &str,
    sys: &systems::System,
    title: &str,
    qid: &str,
) -> Result<(), Error> {
    let qid = qid.trim();
    if !wikidata::is_qid(qid) {
        return Err(Error::Search(format!(
            "{qid:?} is not a Wikidata item id (like Q12345)"
        )));
    }
    conn.execute(
        concat!(
            "INSERT INTO wikidata_match (system_id, title, qid, status, candidates, checked_at)
         VALUES (?1, ?2, ?3, 'by_you', '[]', ",
            crate::db::now!(),
            ")
         ON CONFLICT(system_id, title) DO UPDATE SET qid = excluded.qid, status = 'by_you',
           checked_at = excluded.checked_at"
        ),
        params![sys.id, title, qid],
    )?;
    let mut one = BTreeMap::new();
    one.insert((sys.id, title.to_owned()), Vec::new());
    let r = wikidata::enrich(conn, endpoint, &one, false, |_| {})?;
    if let Some(e) = r.errors.first() {
        return Err(Error::Search(format!(
            "saved, but fetching its details failed: {e}"
        )));
    }
    Ok(())
}
