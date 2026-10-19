//! Emulators and launching.

use crate::{Error, Library, launch};

impl Library {
    /// Where to look for emulators: the real environment, the RomForge
    /// folder's `Emulators/`, and the paths set in Settings.
    pub fn env(&self) -> Result<launch::Env, Error> {
        let mut env = launch::Env::detect();
        if let Some(h) = &self.home {
            env = env.with_emulators_dir(&h.emulators());
        }
        env.user_paths = self.settings()?.emulator_paths;
        Ok(env)
    }

    /// Every emulator romforge could use, and whether it is installed.
    pub fn emulators(&self, env: &launch::Env) -> Result<Vec<launch::EmulatorStatus>, Error> {
        launch::status(&self.conn, env)
    }

    pub fn add_emulator(&mut self, e: &launch::UserEmulator) -> Result<(), Error> {
        launch::add_emulator(&self.conn, e)
    }

    pub fn remove_emulator(&mut self, name: &str) -> Result<usize, Error> {
        launch::remove_emulator(&self.conn, name)
    }

    /// How to start a game (by name or file path); nothing runs yet.
    pub fn launch_plan(
        &self,
        env: &launch::Env,
        query: &str,
        emulator: Option<&str>,
    ) -> Result<launch::LaunchSpec, Error> {
        launch::plan(&self.conn, env, query, emulator)
    }

    /// How to start one library row: exactly its files, never another game
    /// that shares its title.
    pub fn launch_row(
        &self,
        env: &launch::Env,
        row_id: &str,
        emulator: Option<&str>,
    ) -> Result<launch::LaunchSpec, Error> {
        launch::plan_row(&self.conn, env, row_id, emulator)
    }
}
