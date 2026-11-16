//! The Saves page and a game's saves (milestone 27, stage 2;
//! [`crate::saves::page`]).

use crate::saves::gather::GatherPlan;
use crate::saves::page::{self, GameSaves, SystemSaves};
use crate::saves::states::merge;
use crate::{Error, Library, launch};

impl Library {
    /// The Saves page: every save in `Saves/`, one tab per system.
    /// Read-only, like [`Library::list_saves`].
    pub fn saves_page(&self) -> Result<Vec<SystemSaves>, Error> {
        let saves = self.list_saves()?;
        let states = self.list_states()?;
        let v = self.cached_view()?;
        Ok(page::by_system(saves, states, &v))
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
        let states = self.list_states_of(&system)?;
        Ok(page::of_game(&saves, &states, &system, &title))
    }

    /// What moving the save states still in the old `States/` into
    /// `Saves/<system>/states/` would do.
    pub fn merge_states_plan(&self) -> Result<GatherPlan, Error> {
        let home = self.need_home()?;
        let mut plan = merge::plan(home);
        if plan.problem.is_none()
            && let Some(e) = writes_states_running()
        {
            plan.problem = Some(format!("{} is running: close it first", e.label));
        }
        Ok(plan)
    }

    /// Move them (journaled): (journal id, files moved).
    pub fn merge_states(&mut self) -> Result<(i64, usize), Error> {
        let home = self.need_home()?.clone();
        if let Some(e) = writes_states_running() {
            return Err(Error::Launch(format!("{} is running: close it first", e.label)));
        }
        merge::apply(&self.conn, &home)
    }

    /// Put the last move back into `States/`.
    pub fn undo_merge_states(&mut self) -> Result<usize, Error> {
        if let Some(e) = writes_states_running() {
            return Err(Error::Launch(format!("{} is running: close it first", e.label)));
        }
        merge::undo(&self.conn)
    }

    fn need_home(&self) -> Result<&crate::home::Home, Error> {
        self.home
            .as_ref()
            .ok_or_else(|| Error::Settings("no RomForge folder: create one first".into()))
    }
}

/// A running emulator that may write to the old `States/` (RetroArch, the
/// one romforge pointed there).
fn writes_states_running() -> Option<&'static launch::KnownEmulator> {
    launch::known("retroarch").filter(|e| launch::running(e))
}
