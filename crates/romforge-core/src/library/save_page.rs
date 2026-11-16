//! The Saves page and a game's saves (milestone 27, stage 2;
//! [`crate::saves::page`]).

use crate::saves::page::{self, GameSaves, SystemSaves};
use crate::{Error, Library};

impl Library {
    /// The Saves page: every save in `Saves/`, one tab per system.
    /// Read-only, like [`Library::list_saves`].
    pub fn saves_page(&self) -> Result<Vec<SystemSaves>, Error> {
        let saves = self.list_saves()?;
        let v = self.cached_view()?;
        Ok(page::by_system(saves, &v))
    }

    /// The saves of one library row's game (none for a row without a
    /// platform). Reads only that system's folder in `Saves/`.
    pub fn game_saves(&self, id: &str) -> Result<GameSaves, Error> {
        let (system, title) = {
            let v = self.cached_view()?;
            let Some(row) = v.rows.iter().find(|r| r.id == id) else {
                return Err(Error::Dat(format!("no library row {id:?}")));
            };
            let Some(system) = row.system.clone() else {
                return Ok(GameSaves::default());
            };
            (system, row.title.clone())
        };
        let saves = self.list_saves_of(&system)?;
        Ok(page::of_game(&saves, &system, &title))
    }
}
