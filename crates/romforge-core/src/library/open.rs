//! Opening a library, batches, and the plain lookups.

use std::path::{Path, PathBuf};

use crate::{
    DatGame, Error, Library, LibraryFile, LibraryFilter, Summary, SystemInfo, db, extdb, fetch, home,
    identify, patcher, report, settings,
};

/// The settings key marking that DATs changed in a batch and the library
/// still has to be identified again (kept in the database, so a failed
/// batch or a crash doesn't leave stale matches).
const NEEDS_IDENTIFY: &str = "needs_identify";

/// Record, in `conn`'s current transaction, that identify must run again.
pub(super) fn mark_needs_identify(conn: &rusqlite::Connection) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, '1')
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [NEEDS_IDENTIFY],
    )?;
    Ok(())
}

fn needs_identify(conn: &rusqlite::Connection) -> Result<bool, Error> {
    Ok(conn
        .prepare("SELECT 1 FROM settings WHERE key = ?1")?
        .exists([NEEDS_IDENTIFY])?)
}

impl Library {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::open_at(path.as_ref(), true)
    }

    /// `catch_up`: run an identify a DAT batch left due (not for a
    /// connection opened beside the batch itself, [`Library::edit`]).
    pub(super) fn open_at(path: &Path, catch_up: bool) -> Result<Self, Error> {
        let mut conn = db::open(path)?;
        let home = home::Home::of_db(path);
        if let Some(h) = &home {
            home::heal(&mut conn, h.root())?;
        }
        let mut lib = Self {
            conn,
            home,
            path: Some(std::path::absolute(path)?),
            batch_depth: 0,
            batch_dirty: false,
            view_cache: Default::default(),
        };
        // A batch of DAT imports that never got to identify (it failed,
        // or the program stopped): do it now.
        if catch_up && needs_identify(&lib.conn)? {
            lib.identify()?;
        }
        Ok(lib)
    }

    pub fn open_in_memory() -> Result<Self, Error> {
        Ok(Self {
            conn: db::open_in_memory()?,
            path: None,
            home: None,
            batch_depth: 0,
            batch_dirty: false,
            view_cache: Default::default(),
        })
    }

    /// Run `f` with DAT imports deferring the re-identification of scanned
    /// files, then identify once at the end (also when `f` fails, since the
    /// DATs imported before the failure stay). Every import otherwise
    /// re-checks the whole library, which adds up over hundreds of DATs.
    /// Batches nest; only the outermost one identifies.
    pub fn batch<T, E: From<Error>>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, E>) -> Result<T, E> {
        self.batch_depth += 1;
        let result = f(self);
        self.batch_depth -= 1;
        if self.batch_depth == 0 {
            // A TOSEC pack is shared within one batch only.
            fetch::forget_tosec_pack();
            // The marker stays in the database until identify succeeds.
            if std::mem::take(&mut self.batch_dirty) || needs_identify(&self.conn)? {
                self.identify()?;
            }
        }
        result
    }

    /// Recompute all matches, e.g. after removing a DAT.
    pub fn identify(&mut self) -> Result<(), Error> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        identify::identify_all(&tx)?;
        patcher::refresh(&tx)?;
        tx.execute("DELETE FROM settings WHERE key = ?1", [NEEDS_IDENTIFY])?;
        tx.commit()?;
        Ok(())
    }

    pub fn summary(&self) -> Result<Summary, Error> {
        report::summary(&self.conn)
    }

    pub fn dat_games(&self, dat_id: i64) -> Result<Vec<DatGame>, Error> {
        identify::resolve_dat(&self.conn, dat_id)?;
        report::dat_games(&self.conn, dat_id)
    }

    pub fn files(&self, filter: &LibraryFilter) -> Result<Vec<LibraryFile>, Error> {
        report::files(&self.conn, filter)
    }

    /// Where downloaded third-party databases live: the RomForge folder's
    /// `Databases/`, else `databases/` next to the library database; `None`
    /// for an in-memory library.
    pub fn extdb_dir(&self) -> Option<PathBuf> {
        match &self.home {
            Some(h) => Some(h.databases()),
            None => self.path.as_deref().map(extdb::dir_for),
        }
    }

    /// The database file it is open on (none in memory).
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The RomForge folder this library lives in (milestone 13).
    pub fn home(&self) -> Option<&home::Home> {
        self.home.as_ref()
    }

    pub fn settings(&self) -> Result<settings::Settings, Error> {
        settings::load(&self.conn)
    }

    /// Validate and store the settings; returns what was stored.
    pub fn save_settings(&mut self, s: &settings::Settings) -> Result<settings::Settings, Error> {
        settings::save_in(&mut self.conn, s, self.home.is_some())
    }

    /// The settings with their secrets masked, for a form (the app's).
    pub fn settings_masked(&self) -> Result<settings::Settings, Error> {
        let mut s = self.settings()?;
        s.keys = s.keys.masked();
        Ok(s)
    }

    /// Store settings from a form that got [`Library::settings_masked`]:
    /// a secret still showing the mask keeps its stored value. Returns the
    /// stored settings, masked.
    pub fn save_settings_from_form(
        &mut self,
        form: &settings::Settings,
    ) -> Result<settings::Settings, Error> {
        let stored = self.settings()?;
        let mut s = form.clone();
        s.keys = form.keys.clone().unmasked(&stored.keys);
        let mut saved = self.save_settings(&s)?;
        saved.keys = saved.keys.masked();
        Ok(saved)
    }

    pub fn systems_in_use(&self) -> Result<Vec<SystemInfo>, Error> {
        report::systems_in_use(&self.conn)
    }
}
