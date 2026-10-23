//! Pictures, box shapes and release years for the list.

use std::collections::{BTreeMap, HashMap};

use rusqlite::Connection;

use super::GameRow;
use crate::{Error, systems};

/// The art kinds the list's picture views show (and the fullscreen
/// bookshelf's spines).
const LIST_PICTURES: [&str; 5] = ["boxart", "box3d", "hero", "logo", "spine"];

/// Each row's pictures for the list: its own, else its game's.
pub(super) fn pictures(conn: &Connection, rows: &mut [GameRow]) -> Result<(), Error> {
    let mut by: HashMap<(i64, String), BTreeMap<String, String>> = HashMap::new();
    for r in conn
        .prepare("SELECT system_id, title, kind, path FROM art WHERE path IS NOT NULL")?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
    {
        let (s, t, k, p) = r?;
        if LIST_PICTURES.contains(&k.as_str()) {
            by.entry((s, t)).or_default().insert(k, p);
        }
    }
    let bases: HashMap<(i64, String), String> = conn
        .prepare("SELECT system_id, title, base FROM manual_ids WHERE base IS NOT NULL")?
        .query_map([], |r| Ok(((r.get(0)?, r.get(1)?), r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    for r in rows {
        let Some(sid) = r.system_id else { continue };
        if let Some(sys) = systems::by_id(sid) {
            // A patch row's name says nothing of a region: its game's does.
            let named = r.patch_of.as_ref().map_or(r.title.as_str(), |(t, _)| t.as_str());
            let region = systems::BoxRegion::of(&crate::names::parse(named).region_codes);
            r.box_aspect = (systems::box_aspect(sys, region) * 100.0).round() as u16;
            r.box_size = systems::box_size(sys, region);
        }
        let mut own = by.get(&(sid, r.title.clone())).cloned().unwrap_or_default();
        let base = r
            .patch_of
            .as_ref()
            .map(|(t, _)| t.clone())
            .or_else(|| bases.get(&(sid, r.title.clone())).cloned());
        if let Some(b) = base.and_then(|b| by.get(&(sid, b))) {
            for (k, p) in b {
                own.entry(k.clone()).or_insert_with(|| p.clone());
            }
        }
        r.pictures = own;
    }
    Ok(())
}

/// Each row's release year: its own, else (a translation, hack or patch)
/// its game's.
pub(super) fn release_years(conn: &Connection, rows: &mut [GameRow]) -> Result<(), Error> {
    let years: HashMap<(i64, String), i32> = conn
        .prepare("SELECT system_id, title, value FROM game_meta WHERE field = 'releaseyear'")?
        .query_map([], |r| Ok(((r.get(0)?, r.get(1)?), r.get::<_, String>(2)?)))?
        .filter_map(|r| r.ok())
        .filter_map(|(k, v)| Some((k, v.trim().get(..4)?.parse().ok()?)))
        .collect();
    if years.is_empty() {
        return Ok(());
    }
    let bases: HashMap<(i64, String), String> = conn
        .prepare("SELECT system_id, title, base FROM manual_ids WHERE base IS NOT NULL")?
        .query_map([], |r| Ok(((r.get(0)?, r.get(1)?), r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    for r in rows {
        let Some(sid) = r.system_id else { continue };
        let base = r
            .patch_of
            .as_ref()
            .map(|(t, _)| t.clone())
            .or_else(|| bases.get(&(sid, r.title.clone())).cloned());
        r.year = years
            .get(&(sid, r.title.clone()))
            .or_else(|| base.and_then(|b| years.get(&(sid, b))))
            .copied();
    }
    Ok(())
}
