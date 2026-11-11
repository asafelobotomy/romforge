//! The library, opened on the first call and kept while the provider runs,
//! and the game it started, kept until it ends (as the app keeps one): one
//! at a time, its play time recorded when it ends.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use romforge_core::Library;
use romforge_core::launch::{Ended, Playing, Session};
use romforge_core::uq::launcher::Hit;

/// Why a game didn't start, said to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayError {
    /// Not a game in the library Play may start (bad input from the bus).
    Unknown,
    /// No library, a game already running, or the emulator wouldn't start.
    Failed(String),
}

pub struct Games {
    db: PathBuf,
    lib: Mutex<Option<Library>>,
    playing: Mutex<Option<Arc<Session>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Games {
    /// The library at `db`; nothing is opened (or made) until it is needed,
    /// and only an existing database ever is.
    pub fn new(db: PathBuf) -> Self {
        Games {
            db,
            lib: Mutex::new(None),
            playing: Mutex::new(None),
        }
    }

    /// Run `f` on the library, opened first if it isn't yet.
    fn with<T>(&self, f: impl FnOnce(&mut Library) -> Result<T, String>) -> Result<T, String> {
        let mut lib = lock(&self.lib);
        if lib.is_none() {
            if !self.db.is_file() {
                return Err("romforge has no library yet".into());
            }
            // Beside the app's own connections: never the catch-up
            // identify, which is the app's to run.
            let opened = Library::open_beside(&self.db).map_err(|e| format!("{}: {e}", self.db.display()))?;
            *lib = Some(opened);
        }
        f(lib.as_mut().expect("opened just above"))
    }

    /// Open the library and build its list now, so the first search is
    /// quick.
    pub fn warm(&self) -> Result<(), String> {
        self.with(|lib| lib.launcher_search("a").map(drop).map_err(|e| e.to_string()))
    }

    /// The best games for a query; none without a library.
    pub fn search(&self, query: &str) -> Result<Vec<Hit>, String> {
        self.with(|lib| lib.launcher_search(query).map_err(|e| e.to_string()))
    }

    /// Whether a game this provider started is still running.
    pub fn playing(&self) -> bool {
        lock(&self.playing).as_ref().is_some_and(|s| s.is_running())
    }

    /// Start the game `id` names, as the app's Play does: the emulator
    /// romforge picks for it, romforge's config for it written first. The
    /// session is kept; `ended` is called on another thread once it has
    /// ended and its play time is recorded.
    pub fn play(
        self: &Arc<Self>,
        id: &str,
        ended: impl FnOnce(&Ended) + Send + 'static,
    ) -> Result<Playing, PlayError> {
        let mut playing = lock(&self.playing);
        if let Some(s) = playing.as_ref().filter(|s| s.is_running()) {
            return Err(PlayError::Failed(format!(
                "{} is still running: stop it first",
                s.playing.game
            )));
        }
        let spec = self
            .with(|lib| {
                if lib.launcher_game(id).map_err(|e| e.to_string())?.is_none() {
                    return Ok(None);
                }
                let env = lib.env().map_err(|e| e.to_string())?;
                lib.launch_row(&env, id, None)
                    .map(Some)
                    .map_err(|e| e.to_string())
            })
            .map_err(PlayError::Failed)?
            .ok_or(PlayError::Unknown)?;
        let session = Arc::new(Session::start(&spec).map_err(|e| PlayError::Failed(e.to_string()))?);
        let started = session.playing.clone();
        *playing = Some(session.clone());
        let games = self.clone();
        std::thread::spawn(move || {
            let end = session.wait();
            if let Err(e) = games.with(|lib| lib.record_play(&end).map_err(|e| e.to_string())) {
                eprintln!(
                    "romforge-provider: {}'s play time wasn't kept: {e}",
                    end.playing.game
                );
            }
            ended(&end);
        });
        Ok(started)
    }
}
