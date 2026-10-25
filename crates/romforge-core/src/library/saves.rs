//! Saves (milestone 27): gathering them into `Saves/`, and the backup
//! copies of the ones that stay elsewhere.

use std::path::{Path, PathBuf};

use super::emulators::not_running;
use crate::saves::{backup, gather};
use crate::{Error, Library, launch};

/// Seconds since 1970, now.
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Library {
    /// What gathering `emulator`'s saves into `Saves/` would do.
    pub fn gather_saves_plan(&self, emulator: &str) -> Result<gather::GatherPlan, Error> {
        let (home, e, _) = self.setup_target(emulator)?;
        let user = self.env()?.user;
        let mut plan = gather::plan(&self.conn, e.settings_name(), home, user.as_ref());
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
        gather::apply(&self.conn, e.settings_name(), home, user.as_ref())
    }

    /// Put the last gathering of `emulator`'s saves back.
    pub fn undo_gather_saves(&mut self, emulator: &str) -> Result<usize, Error> {
        let (_, e, _) = self.setup_target(emulator)?;
        not_running(e)?;
        gather::undo(&self.conn, e.settings_name())
    }

    /// Copy `emulator`'s saves that live outside `Saves/` (each changed
    /// one; refused while it runs).
    pub fn backup_saves(&self, emulator: &str) -> Result<backup::BackedUp, Error> {
        let (home, e, _) = self.setup_target(emulator)?;
        not_running(e)?;
        backup::run(home, e.settings_name(), self.env()?.user.as_ref(), now())
    }

    /// After a game of `emulator` ended: its saves outside `Saves/` copied.
    /// A failure doesn't fail the end of the game; it shows on the next
    /// backup asked for.
    pub(super) fn backup_after_play(&self, emulator: &str) {
        let (Some(home), Some(e)) = (&self.home, launch::known(emulator)) else {
            return;
        };
        if backup::backs_up(e.settings_name()) {
            let user = self.env().ok().and_then(|env| env.user);
            let _ = backup::run(home, e.settings_name(), user.as_ref(), now());
        }
    }

    /// `emulator`'s save folders outside `Saves/` and their copies.
    pub fn save_backups(&self, emulator: &str) -> Result<Vec<backup::Backed>, Error> {
        let (home, e, _) = self.setup_target(emulator)?;
        Ok(backup::list(home, e.settings_name(), self.env()?.user.as_ref()))
    }

    /// What putting `save` back from `version` (else its newest copy)
    /// would do.
    pub fn restore_save_plan(
        &self,
        emulator: &str,
        save: &Path,
        version: Option<&str>,
    ) -> Result<backup::restore::RestorePlan, Error> {
        let (home, e, _) = self.setup_target(emulator)?;
        let user = self.env()?.user;
        let mut plan = backup::restore::plan(home, e.settings_name(), user.as_ref(), save, version);
        if plan.problem.is_none() && launch::running(e) {
            plan.problem = Some(format!("{} is running: close it first", e.label));
        }
        Ok(plan)
    }

    /// Put `save` back from its copy (the one there now to the trash).
    pub fn restore_save(&mut self, emulator: &str, save: &Path, version: Option<&str>) -> Result<i64, Error> {
        let (home, e, _) = self.setup_target(emulator)?;
        not_running(e)?;
        let user = self.env()?.user;
        backup::restore::apply(&self.conn, home, e.settings_name(), user.as_ref(), save, version)
    }

    /// Take back the last restore of `emulator`'s saves.
    pub fn undo_restore_save(&mut self, emulator: &str) -> Result<PathBuf, Error> {
        let (home, e, _) = self.setup_target(emulator)?;
        not_running(e)?;
        backup::restore::undo(&self.conn, home, e.settings_name())
    }
}
