//! Stage 2, step 6: what the user keeps per game, by system key and title
//! (as play time is): a favourite, and the emulator picked for it. Kept in
//! `user_games`, apart from what Enrich gathers, so nothing romforge
//! fetches ever overwrites it.

use rusqlite::{Connection, OptionalExtension, params};

use crate::Error;
use crate::launch::{Env, Played, play_summary, status};

const FAVOURITE: &str = "favourite";
const EMULATOR: &str = "emulator";

fn get(conn: &Connection, system: &str, title: &str, key: &str) -> Result<Option<String>, Error> {
    Ok(conn
        .query_row(
            "SELECT value FROM user_games WHERE system = ?1 AND title = ?2 AND key = ?3",
            params![system, title, key],
            |r| r.get(0),
        )
        .optional()?)
}

pub(crate) fn set(
    conn: &Connection,
    system: &str,
    title: &str,
    key: &str,
    value: Option<&str>,
) -> Result<(), Error> {
    match value {
        Some(v) => conn.execute(
            "INSERT INTO user_games (system, title, key, value) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (system, title, key) DO UPDATE SET value = excluded.value",
            params![system, title, key, v],
        )?,
        None => conn.execute(
            "DELETE FROM user_games WHERE system = ?1 AND title = ?2 AND key = ?3",
            params![system, title, key],
        )?,
    };
    Ok(())
}

pub fn set_favourite(conn: &Connection, system: &str, title: &str, on: bool) -> Result<(), Error> {
    set(conn, system, title, FAVOURITE, on.then_some("1"))
}

/// The emulator picked for a game (its name as `launch` takes it:
/// `retroarch:<core>`, a known emulator's or one the user added).
pub fn emulator(conn: &Connection, system: &str, title: &str) -> Result<Option<String>, Error> {
    get(conn, system, title, EMULATOR)
}

/// Pick `name` for a game (one of [`emulator_choices`]), or none to let
/// romforge choose again.
pub fn set_emulator(
    conn: &Connection,
    env: &Env,
    system: &str,
    title: &str,
    name: Option<&str>,
) -> Result<(), Error> {
    if let Some(n) = name
        && !emulator_choices(conn, env, system)?.iter().any(|c| c == n)
    {
        return Err(Error::Launch(format!(
            "{n} isn't an installed emulator for {system}"
        )));
    }
    set(conn, system, title, EMULATOR, name)
}

/// The installed emulators that play `system`, in the order romforge
/// tries them.
pub fn emulator_choices(conn: &Connection, env: &Env, system: &str) -> Result<Vec<String>, Error> {
    Ok(status(conn, env)?
        .into_iter()
        .filter(|e| e.found.is_some() && e.systems.iter().any(|s| s == system))
        .map(|e| e.name)
        .collect())
}

/// The favourites, and every game's play time.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Marks {
    pub favourites: Vec<(String, String)>,
    pub played: Vec<Played>,
}

pub fn marks(conn: &Connection) -> Result<Marks, Error> {
    let mut stmt =
        conn.prepare_cached("SELECT system, title FROM user_games WHERE key = ?1 ORDER BY system, title")?;
    let favourites = stmt
        .query_map([FAVOURITE], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    Ok(Marks {
        favourites,
        played: play_summary(conn)?,
    })
}

#[cfg(test)]
mod tests;
