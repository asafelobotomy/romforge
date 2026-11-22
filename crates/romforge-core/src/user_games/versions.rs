//! Play on a game with several versions (other regions, revisions,
//! editions, translations, hacks): each playable one is offered, and the
//! one played with "Always run" ticked starts straight away next time.
//! Kept in `user_games` under the version group's key (as its `title`) and
//! the group's system key, by the version's title: a rescan keeps titles,
//! not row ids.

use rusqlite::Connection;

use super::{get, set};
use crate::Error;
use crate::view::GameRow;

const ALWAYS_RUN: &str = "always_run";

/// What Play does with a row.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PlayChoice {
    /// The row to start now: the one asked for when there is nothing to
    /// pick, or the version kept with "Always run".
    pub run: Option<String>,
    /// Otherwise the versions to pick from (two or more, all playable),
    /// the group's main one first, then as the list shows them.
    pub versions: Vec<GameRow>,
}

/// One "Always run" choice, for `always-run list`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AlwaysRun {
    pub system: String,
    pub group: String,
    /// The version's title.
    pub title: String,
}

fn row<'a>(rows: &'a [GameRow], id: &str) -> Result<&'a GameRow, Error> {
    rows.iter()
        .find(|r| r.id == id)
        .ok_or_else(|| Error::Launch(format!("no library row {id:?}")))
}

/// The playable versions of `r`'s group, main first, then by category in
/// the list's order and title. Empty when `r` has no group.
pub fn playable_versions<'a>(rows: &'a [GameRow], r: &GameRow) -> Vec<&'a GameRow> {
    let Some(group) = &r.group else {
        return Vec::new();
    };
    let mut out: Vec<&GameRow> = rows
        .iter()
        .filter(|m| m.group.as_ref() == Some(group) && m.playable)
        .collect();
    // `Category` sorts in the list's order; the main row has none.
    out.sort_by(|a, b| (!a.main, a.category, &a.title).cmp(&(!b.main, b.category, &b.title)));
    out
}

/// The version kept with "Always run" for `r`'s group, if one was and it
/// is still here and playable.
pub fn always_run<'a>(
    conn: &Connection,
    rows: &'a [GameRow],
    r: &GameRow,
) -> Result<Option<&'a GameRow>, Error> {
    let (Some(system), Some(group)) = (&r.system, &r.group) else {
        return Ok(None);
    };
    let Some(title) = get(conn, system, group, ALWAYS_RUN)? else {
        return Ok(None);
    };
    Ok(playable_versions(rows, r).into_iter().find(|v| v.title == title))
}

/// What Play does with row `id`: a version picked on its own (not its
/// group's main row), or a game with one playable version, starts as it
/// is; a game with a version kept by "Always run" starts that one; any
/// other game with versions asks which.
pub fn play_choice(conn: &Connection, rows: &[GameRow], id: &str) -> Result<PlayChoice, Error> {
    let r = row(rows, id)?;
    let start = |id: &str| PlayChoice {
        run: Some(id.to_owned()),
        versions: Vec::new(),
    };
    if !r.main || r.group.is_none() {
        return Ok(start(id));
    }
    if let Some(v) = always_run(conn, rows, r)? {
        return Ok(start(&v.id));
    }
    let versions = playable_versions(rows, r);
    if versions.len() < 2 {
        return Ok(start(versions.first().map_or(id, |v| v.id.as_str())));
    }
    Ok(PlayChoice {
        run: None,
        versions: versions.into_iter().cloned().collect(),
    })
}

/// Keep `version` (a playable row in `id`'s group) as the one Play always
/// runs for that game, or with `None` ask again.
pub fn set_always_run(
    conn: &Connection,
    rows: &[GameRow],
    id: &str,
    version: Option<&str>,
) -> Result<(), Error> {
    let r = row(rows, id)?;
    let (Some(system), Some(group)) = (&r.system, &r.group) else {
        return Err(Error::Launch(format!("{} has no other version", r.title)));
    };
    let title = match version {
        None => None,
        Some(v) => {
            let v = row(rows, v)?;
            if v.group.as_ref() != Some(group) || !v.playable {
                return Err(Error::Launch(format!(
                    "{} isn't a playable version of {}",
                    v.title, r.title
                )));
            }
            Some(v.title.as_str())
        }
    };
    set(conn, system, group, ALWAYS_RUN, title)
}

/// Every "Always run" choice kept, by system and group.
pub fn always_run_list(conn: &Connection) -> Result<Vec<AlwaysRun>, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT system, title, value FROM user_games WHERE key = ?1 ORDER BY system, title",
    )?;
    let rows = stmt.query_map([ALWAYS_RUN], |r| {
        Ok(AlwaysRun {
            system: r.get(0)?,
            group: r.get(1)?,
            title: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod tests;
