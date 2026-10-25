//! Settings for one game (milestone 21), for the emulators that read a
//! file of a game's own over their usual settings: RetroArch (a game
//! override, `config/<core>/<game>.cfg`, read after `--appendconfig`) and
//! DuckStation (`gamesettings/<serial>.ini`). Kept per game in
//! `user_games` as `emu.<setting>`; a setting not set there is the
//! emulator's.

use rusqlite::Connection;

use super::{Key, SETTINGS, Setting, Values, checked};
use crate::Error;

const PREFIX: &str = "emu.";

/// The emulators with settings of a game's own.
pub const PER_GAME: &[&str] = &["retroarch", "duckstation"];

/// Whether `s` can be set for one game.
fn per_game(s: &Setting) -> bool {
    s.keys.iter().any(|k| PER_GAME.contains(&k.emulator))
}

/// A setting one game can have.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GameSetting {
    pub setting: &'static Setting,
    /// The emulators for its system it reaches.
    pub emulators: Vec<&'static str>,
}

/// What a game's "Settings for this game" shows.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GameSettingsView {
    pub settings: Vec<GameSetting>,
    pub values: Values,
}

/// The settings a game of `system` can have and what it has.
pub fn game_settings_view(conn: &Connection, system: &str, title: &str) -> Result<GameSettingsView, Error> {
    let sys = crate::systems::by_key(system).ok_or_else(|| Error::UnknownSystem(system.to_owned()))?;
    let runs = |e: &str| {
        if e == "retroarch" {
            !sys.cores.is_empty()
        } else {
            sys.emulators.contains(&e)
        }
    };
    let settings = SETTINGS
        .iter()
        .filter_map(|s| {
            let mut emulators: Vec<&str> = s
                .keys
                .iter()
                .map(|k| k.emulator)
                .filter(|e| PER_GAME.contains(e) && runs(e))
                .collect();
            emulators.dedup();
            (!emulators.is_empty()).then_some(GameSetting {
                setting: s,
                emulators,
            })
        })
        .collect();
    Ok(GameSettingsView {
        settings,
        values: game_settings(conn, system, title)?,
    })
}

/// What is set for one game.
pub fn game_settings(conn: &Connection, system: &str, title: &str) -> Result<Values, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT key, value FROM user_games WHERE system = ?1 AND title = ?2 AND key LIKE 'emu.%'",
    )?;
    let rows = stmt.query_map([system, title], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut out = Values::new();
    for row in rows {
        let (k, v) = row?;
        if let Some(k) = k.strip_prefix(PREFIX) {
            out.insert(k.to_owned(), v);
        }
    }
    Ok(out)
}

/// Set one game's settings in place of what it had (empty values: not
/// set). Returns what was stored.
pub fn set_game_settings(
    conn: &Connection,
    system: &str,
    title: &str,
    values: &Values,
) -> Result<Values, Error> {
    let mut kept = Values::new();
    for (key, value) in values {
        let def = super::setting(key)
            .filter(|s| per_game(s))
            .ok_or_else(|| Error::Settings(format!("{key:?} can't be set for one game")))?;
        if let Some(v) = checked(def, value).map_err(Error::Settings)? {
            kept.insert(key.clone(), v);
        }
    }
    for old in game_settings(conn, system, title)?.keys() {
        crate::user_games::set(conn, system, title, &format!("{PREFIX}{old}"), None)?;
    }
    for (k, v) in &kept {
        crate::user_games::set(conn, system, title, &format!("{PREFIX}{k}"), Some(v))?;
    }
    Ok(kept)
}

/// `emulator`'s keys of the settings a game can have, each with the game's
/// value in the emulator's words (`None`: not set for it, or a value the
/// emulator lacks).
pub(crate) fn game_keys(
    conn: &Connection,
    system: &str,
    title: &str,
    emulator: &str,
) -> Result<Vec<(&'static Key, Option<String>)>, Error> {
    let set = game_settings(conn, system, title)?;
    Ok(SETTINGS
        .iter()
        .filter(|s| per_game(s))
        .flat_map(|s| {
            s.keys
                .iter()
                .filter(|k| k.emulator == emulator)
                .map(move |k| (s, k))
        })
        .map(|(s, k)| (k, set.get(s.key).and_then(|v| k.words.of(v))))
        .collect())
}
