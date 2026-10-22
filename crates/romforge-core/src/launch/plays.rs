//! Play time (milestone 18, 1.6): each start romforge made, how long the
//! emulator ran, kept by system key and game title.

use rusqlite::{Connection, OptionalExtension, params};

use super::Ended;
use crate::Error;

/// How much a game has been played.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PlayStats {
    pub seconds: u64,
    pub sessions: u64,
    /// Seconds since 1970 when it was last started.
    pub last_played: u64,
}

/// Starts shorter than this (an emulator that failed to start, a game
/// quit at once) aren't counted.
const MIN_SECONDS: u64 = 10;

/// Record a game that ended; a very short run isn't one. Returns whether
/// it was recorded.
pub fn record_play(conn: &Connection, e: &Ended) -> Result<bool, Error> {
    if e.seconds < MIN_SECONDS {
        return Ok(false);
    }
    conn.execute(
        "INSERT INTO play_sessions (system, game, emulator, started_at, seconds) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            e.playing.system,
            e.playing.game,
            e.playing.emulator,
            e.playing.started,
            e.seconds
        ],
    )?;
    Ok(true)
}

/// How much `game` on `system` has been played; `None` never.
pub fn play_stats(conn: &Connection, system: &str, game: &str) -> Result<Option<PlayStats>, Error> {
    Ok(conn
        .query_row(
            "SELECT SUM(seconds), COUNT(*), MAX(started_at) FROM play_sessions WHERE system = ?1 AND game = ?2
             HAVING COUNT(*) > 0",
            params![system, game],
            |r| {
                Ok(PlayStats {
                    seconds: r.get::<_, i64>(0)? as u64,
                    sessions: r.get::<_, i64>(1)? as u64,
                    last_played: r.get::<_, i64>(2)? as u64,
                })
            },
        )
        .optional()?)
}

/// One game's play time, for the lists of games played.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Played {
    pub system: String,
    pub title: String,
    pub seconds: u64,
    pub sessions: u64,
    pub last_played: u64,
}

/// Every game played, the most recent first.
pub fn play_summary(conn: &Connection) -> Result<Vec<Played>, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT system, game, SUM(seconds), COUNT(*), MAX(started_at) FROM play_sessions
         GROUP BY system, game ORDER BY MAX(started_at) DESC, game",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Played {
            system: r.get(0)?,
            title: r.get(1)?,
            seconds: r.get::<_, i64>(2)? as u64,
            sessions: r.get::<_, i64>(3)? as u64,
            last_played: r.get::<_, i64>(4)? as u64,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}
