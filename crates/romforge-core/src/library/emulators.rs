//! Emulators and launching.

use crate::{Error, Library, emusettings, firmware, launch};

impl Library {
    /// Where to look for emulators and how to start them: the real
    /// environment, the RomForge folder's `Emulators/`, and Emulator
    /// Settings.
    pub fn env(&self) -> Result<launch::Env, Error> {
        let mut env = launch::Env::detect();
        if let Some(h) = &self.home {
            env = env.with_emulators_dir(&h.emulators());
            env.home = Some(h.clone());
        }
        emusettings::apply(&emusettings::load(&self.conn)?, &mut env);
        Ok(env)
    }

    /// What the Emulator Settings dialog shows.
    pub fn emulator_settings(&self) -> Result<emusettings::EmulatorSettingsView, Error> {
        emusettings::view(&self.conn, &self.env()?)
    }

    /// Fetch RetroBIOS's list of what emulators need (milestone 20).
    pub fn firmware_download(&self) -> Result<usize, Error> {
        let dir = self
            .extdb_dir()
            .ok_or_else(|| Error::Settings("no folder for downloaded lists".into()))?;
        std::fs::create_dir_all(&dir)?;
        firmware::download(&dir)
    }

    /// Which BIOS files each system's emulators need, and which are in the
    /// RomForge folder's `Bios/`.
    pub fn firmware_report(&self) -> Result<firmware::FirmwareReport, Error> {
        let home = self.home.as_ref().ok_or_else(|| {
            Error::Settings("BIOS files are checked in a RomForge folder; create one first".into())
        })?;
        let dir = self.extdb_dir().unwrap_or_else(|| home.databases());
        firmware::report(&dir, home, &self.env()?)
    }

    pub fn save_emulator_settings(
        &mut self,
        s: &emusettings::EmulatorSettings,
    ) -> Result<emusettings::EmulatorSettings, Error> {
        emusettings::save(&mut self.conn, s)
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
