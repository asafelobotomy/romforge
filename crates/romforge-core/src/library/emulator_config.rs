//! Milestone 21: every setting in romforge's copy of an emulator's config
//! (All settings), and settings for one game.

use crate::emusettings::{self, raw};
use crate::{Error, Library, launch};

impl Library {
    /// What All settings shows for `emulator`.
    pub fn raw_settings(&self, emulator: &str) -> Result<raw::RawView, Error> {
        raw::view(&self.conn, &self.env()?, emulator)
    }

    /// Change keys in `emulator`'s config by hand (one batch for undo).
    pub fn edit_raw_settings(&mut self, emulator: &str, edits: &[raw::RawEdit]) -> Result<i64, Error> {
        let env = self.env()?;
        raw::edit(&mut self.conn, &env, emulator, edits)
    }

    /// Take back the last batch of those changes.
    pub fn undo_raw_settings(&mut self, emulator: &str) -> Result<usize, Error> {
        raw::undo(&self.conn, &self.env()?, emulator)
    }

    /// Have MAME write all its settings into romforge's `mame.ini`.
    pub fn mame_write_defaults(&self) -> Result<std::path::PathBuf, Error> {
        launch::mame_write_defaults(&self.env()?)
    }

    /// The settings a game can have of its own, and what it has.
    pub fn game_settings(&self, system: &str, title: &str) -> Result<emusettings::GameSettingsView, Error> {
        emusettings::game_settings_view(&self.conn, system, title)
    }

    pub fn set_game_settings(
        &mut self,
        system: &str,
        title: &str,
        values: &emusettings::Values,
    ) -> Result<emusettings::Values, Error> {
        emusettings::set_game_settings(&self.conn, system, title, values)
    }
}
