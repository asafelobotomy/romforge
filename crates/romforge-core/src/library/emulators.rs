//! Emulators and launching.

use crate::{Error, Library, emusettings, firmware, launch, obtain};

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
        env.firmware_list = self.firmware_dirs().ok().map(|(_, dir)| dir);
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
        let n = firmware::download(&dir)?;
        firmware::index(&self.conn, &dir)?;
        Ok(n)
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
        firmware::report(&self.conn, &dir, home, &self.env()?)
    }

    /// The library's BIOS badges ([`firmware::FirmwareReport::needs`]); none
    /// without a RomForge folder or RetroBIOS's list.
    pub fn firmware_needs(&self) -> Vec<firmware::BiosNeed> {
        self.firmware_report().map(|r| r.needs).unwrap_or_default()
    }

    /// The scanned files that are BIOS files (milestone 20, step 6; a
    /// member of an archive by its archive), all of them or only the file
    /// `file_id`, for an import plan like a drop's.
    pub fn firmware_in_library(&self, file_id: Option<i64>) -> Result<Vec<std::path::PathBuf>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT f.path FROM files f JOIN firmware_hashes h ON h.size = f.size
               AND (h.sha1 = lower(f.sha1) OR h.md5 = lower(f.md5))
             WHERE f.status = 'unknown' AND (?1 IS NULL OR f.file_id = ?1) ORDER BY f.path",
        )?;
        let paths = stmt
            .query_map([file_id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(paths.into_iter().map(std::path::PathBuf::from).collect())
    }

    /// The usual BIOS places that exist on this computer (RetroArch's
    /// system folder, EmuDeck's, RetroDECK's, the emulators' own), for an
    /// import plan like a drop's.
    pub fn firmware_found(&self) -> Result<Vec<std::path::PathBuf>, Error> {
        let env = self.env()?;
        Ok(env
            .user
            .as_ref()
            .map(|u| {
                firmware::find::usual_places(u)
                    .into_iter()
                    .map(|(_, p)| p)
                    .collect()
            })
            .unwrap_or_default())
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
        let move_originals = !crate::settings::load(&self.conn)?.bios_leave_in_place;
        firmware::apply(&self.conn, &dir, home, &self.env()?, paths, also, move_originals)
    }

    /// Install the PS3 firmware in `Bios/rpcs3/` into RPCS3.
    pub fn install_ps3_firmware(&self) -> Result<std::path::PathBuf, Error> {
        launch::install_ps3_firmware(&self.env()?)
    }

    /// The cores romforge's platforms use, and what the buildbot has.
    pub fn emulator_cores(&self) -> Result<obtain::CoreList, Error> {
        let (home, dir) = self.firmware_dirs()?;
        Ok(obtain::cores(&dir, home, &self.env()?))
    }

    /// Fetch the buildbot's list of cores and libretro's core info.
    pub fn refresh_emulator_cores(&self) -> Result<usize, Error> {
        let (_, dir) = self.firmware_dirs()?;
        obtain::refresh_cores(&dir)
    }

    /// Download a core into the RomForge folder's `Emulators/cores/`. A
    /// core whose licence forbids commercial use is refused unless
    /// `accept_non_commercial` (the user has been shown it and agreed).
    pub fn download_core(&self, core: &str, accept_non_commercial: bool) -> Result<String, Error> {
        if !accept_non_commercial
            && let Some(c) = self
                .emulator_cores()?
                .cores
                .iter()
                .find(|c| c.name == core && c.non_commercial)
        {
            return Err(Error::Launch(format!(
                "{core}'s licence ({}) forbids commercial use; it is downloaded only once that is accepted",
                c.license
            )));
        }
        let (home, dir) = self.firmware_dirs()?;
        obtain::download_core(&dir, home, core)
    }

    /// Remove a core romforge downloaded.
    pub fn remove_core(&self, core: &str) -> Result<(), Error> {
        let (home, _) = self.firmware_dirs()?;
        obtain::remove_core(home, core)
    }

    /// Install an emulator from Flathub into the user's Flatpak installation.
    /// Download `emulator`'s official AppImage into the RomForge folder's
    /// `Emulators/` (RPCS3's). Returns where it went.
    pub fn install_appimage(&self, emulator: &str) -> Result<std::path::PathBuf, Error> {
        let home = self.home.as_ref().ok_or_else(|| {
            Error::Settings("AppImages are kept in a RomForge folder; create one first".into())
        })?;
        obtain::install_appimage(home, emulator)
    }

    pub fn install_emulator(&self, emulator: &str) -> Result<String, Error> {
        obtain::install_flatpak(&self.env()?, emulator)
    }

    /// Record a game that ended (play time); a run under ten seconds isn't
    /// counted.
    pub fn record_play(&mut self, ended: &launch::Ended) -> Result<bool, Error> {
        launch::record_play(&self.conn, ended)
    }

    /// How much a game (system key, title) has been played.
    pub fn play_stats(&self, system: &str, game: &str) -> Result<Option<launch::PlayStats>, Error> {
        launch::play_stats(&self.conn, system, game)
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
        not_running(e)?;
        let xdg = launch::existing::Xdg::detect(&user);
        launch::existing::import(&self.conn, e, home, &user, &xdg)
    }

    /// What moving `emulator`'s game data into RomForge would do.
    pub fn move_data_plan(&self, emulator: &str) -> Result<launch::move_in::MovePlan, Error> {
        let (home, e, _) = self.setup_target(emulator)?;
        let mut plan = launch::move_in::plan(e.settings_name(), home, self.env()?.user.as_ref());
        if plan.problem.is_none() && launch::running(e) {
            plan.problem = Some(format!("{} is running: close it first", e.label));
        }
        Ok(plan)
    }

    /// Move `emulator`'s game data into RomForge (originals to the trash).
    pub fn move_data(&mut self, emulator: &str) -> Result<i64, Error> {
        let (home, e, _) = self.setup_target(emulator)?;
        not_running(e)?;
        launch::move_in::apply(&self.conn, e.settings_name(), home, self.env()?.user.as_ref())
    }

    /// Take back the last move of `emulator`'s game data.
    pub fn undo_move_data(&mut self, emulator: &str) -> Result<usize, Error> {
        let (home, e, _) = self.setup_target(emulator)?;
        not_running(e)?;
        launch::move_in::undo(&self.conn, e.settings_name(), home)
    }

    /// What gathering `emulator`'s saves into `Saves/` would do.
    pub fn gather_saves_plan(&self, emulator: &str) -> Result<crate::saves::gather::GatherPlan, Error> {
        let (home, e, _) = self.setup_target(emulator)?;
        let user = self.env()?.user;
        let mut plan = crate::saves::gather::plan(&self.conn, e.settings_name(), home, user.as_ref());
        if plan.problem.is_none() && launch::running(e) {
            plan.problem = Some(format!("{} is running: close it first", e.label));
        }
        Ok(plan)
    }

    /// Gather `emulator`'s saves into `Saves/`: (gathering id, files moved).
    pub fn gather_saves(&mut self, emulator: &str) -> Result<(i64, usize), Error> {
        let (home, e, _) = self.setup_target(emulator)?;
        not_running(e)?;
        let user = self.env()?.user;
        crate::saves::gather::apply(&self.conn, e.settings_name(), home, user.as_ref())
    }

    /// Put the last gathering of `emulator`'s saves back.
    pub fn undo_gather_saves(&mut self, emulator: &str) -> Result<usize, Error> {
        let (_, e, _) = self.setup_target(emulator)?;
        not_running(e)?;
        crate::saves::gather::undo(&self.conn, e.settings_name())
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

    /// Whether romforge's game pad hotkeys are on (Emulator Settings).
    pub fn hotkeys_on(&self) -> Result<bool, Error> {
        Ok(emusettings::hotkeys_on(&emusettings::load(&self.conn)?))
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
    /// The favourites and every game's play time (fullscreen mode).
    pub fn marks(&self) -> Result<crate::user_games::Marks, Error> {
        crate::user_games::marks(&self.conn)
    }

    pub fn set_favourite(&self, system: &str, title: &str, on: bool) -> Result<(), Error> {
        crate::user_games::set_favourite(&self.conn, system, title, on)
    }

    /// The emulator picked for a game, and the ones it could be.
    pub fn game_emulator(
        &self,
        env: &launch::Env,
        system: &str,
        title: &str,
    ) -> Result<(Option<String>, Vec<String>), Error> {
        Ok((
            crate::user_games::emulator(&self.conn, system, title)?,
            crate::user_games::emulator_choices(&self.conn, env, system)?,
        ))
    }

    pub fn set_game_emulator(
        &self,
        env: &launch::Env,
        system: &str,
        title: &str,
        name: Option<&str>,
    ) -> Result<(), Error> {
        crate::user_games::set_emulator(&self.conn, env, system, title, name)
    }

    pub fn launch_row(
        &self,
        env: &launch::Env,
        row_id: &str,
        emulator: Option<&str>,
    ) -> Result<launch::LaunchSpec, Error> {
        launch::plan_row(&self.conn, env, row_id, emulator)
    }
}

/// Refuse while `e` is running: its files must not change under it.
fn not_running(e: &launch::KnownEmulator) -> Result<(), Error> {
    if launch::running(e) {
        return Err(Error::Launch(format!("{} is running: close it first", e.label)));
    }
    Ok(())
}
