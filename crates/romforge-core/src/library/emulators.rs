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

    /// The RomForge folder and the downloaded lists' folder, for BIOS work.
    fn firmware_dirs(&self) -> Result<(&crate::home::Home, std::path::PathBuf), Error> {
        let home = self.home.as_ref().ok_or_else(|| {
            Error::Settings("BIOS files are kept in a RomForge folder; create one first".into())
        })?;
        Ok((home, self.extdb_dir().unwrap_or_else(|| home.databases())))
    }

    /// Which BIOS files each system's emulators need, and which are in the
    /// RomForge folder's `Bios/`.
    pub fn firmware_report(&self) -> Result<firmware::FirmwareReport, Error> {
        let (home, dir) = self.firmware_dirs()?;
        firmware::report(&dir, home, &self.env()?)
    }

    /// What importing the BIOS files in `paths` would do; nothing is written.
    pub fn firmware_import_plan(&self, paths: &[std::path::PathBuf]) -> Result<firmware::ImportPlan, Error> {
        let (home, dir) = self.firmware_dirs()?;
        firmware::plan(&dir, home, &self.env()?, paths)
    }

    /// Import them: accepted files, and the name-only ones in `also`.
    pub fn firmware_import(
        &mut self,
        paths: &[std::path::PathBuf],
        also: &[String],
    ) -> Result<firmware::Imported, Error> {
        let (home, dir) = self.firmware_dirs()?;
        firmware::apply(&self.conn, &dir, home, &self.env()?, paths, also)
    }

    /// Take away what the last BIOS import wrote.
    pub fn firmware_undo(&mut self) -> Result<usize, Error> {
        firmware::undo(&self.conn)
    }

    /// Copy `emulator`'s setup from its usual folders into romforge's.
    pub fn import_emulator_setup(
        &mut self,
        emulator: &str,
    ) -> Result<launch::existing::SetupImported, Error> {
        let (home, e, user) = self.setup_target(emulator)?;
        let xdg = launch::existing::Xdg::detect(&user);
        launch::existing::import(&self.conn, e, home, &user, &xdg)
    }

    /// Take back the last setup import of `emulator`.
    pub fn undo_emulator_setup(&mut self, emulator: &str) -> Result<usize, Error> {
        let (_, e, _) = self.setup_target(emulator)?;
        launch::existing::undo(&self.conn, e.settings_name())
    }

    fn setup_target(
        &self,
        emulator: &str,
    ) -> Result<
        (
            &crate::home::Home,
            &'static launch::KnownEmulator,
            std::path::PathBuf,
        ),
        Error,
    > {
        let home = self.home.as_ref().ok_or_else(|| {
            Error::Settings("romforge keeps emulator setups in a RomForge folder; create one first".into())
        })?;
        let e = launch::known(emulator)
            .filter(|e| launch::existing::importable(e.settings_name()))
            .ok_or_else(|| Error::Launch(format!("romforge doesn't set up {emulator:?}")))?;
        let user = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .ok_or_else(|| Error::Settings("$HOME is not set".into()))?;
        Ok((home, e, user))
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
