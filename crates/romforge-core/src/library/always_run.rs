//! Play on a game with several versions, and "Always run"
//! ([`crate::user_games::versions`]).

use crate::user_games::versions::{self, AlwaysRun, PlayChoice};
use crate::{Error, Library, view};

impl Library {
    /// What Play does with library row `id`: start a row now, or offer
    /// its game's versions.
    pub fn play_choice(&self, id: &str) -> Result<PlayChoice, Error> {
        let v = self.cached_view()?;
        versions::play_choice(&self.conn, &v.rows, id)
    }

    /// The playable versions of row `id`'s game, its main one first (none
    /// when it has no other version).
    pub fn playable_versions(&self, id: &str) -> Result<Vec<view::GameRow>, Error> {
        let v = self.cached_view()?;
        let Some(r) = v.rows.iter().find(|r| r.id == id) else {
            return Ok(Vec::new());
        };
        Ok(versions::playable_versions(&v.rows, r)
            .into_iter()
            .cloned()
            .collect())
    }

    /// The version Play always runs for row `id`'s game, if one is kept.
    pub fn always_run(&self, id: &str) -> Result<Option<view::GameRow>, Error> {
        let v = self.cached_view()?;
        let Some(r) = v.rows.iter().find(|r| r.id == id) else {
            return Ok(None);
        };
        Ok(versions::always_run(&self.conn, &v.rows, r)?.cloned())
    }

    /// Keep `version` as the one Play always runs for row `id`'s game, or
    /// with `None` have Play ask again.
    pub fn set_always_run(&self, id: &str, version: Option<&str>) -> Result<(), Error> {
        let v = self.cached_view()?;
        versions::set_always_run(&self.conn, &v.rows, id, version)
    }

    /// Every game's "Always run" choice.
    pub fn always_run_list(&self) -> Result<Vec<AlwaysRun>, Error> {
        versions::always_run_list(&self.conn)
    }
}
