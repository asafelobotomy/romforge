//! The library, opened on the first call and kept while the provider runs,
//! and the game it started, kept until it ends (as the app keeps one): one
//! at a time, its play time recorded when it ends. While it runs, the
//! provider is the controller layer the app is for its own games: it reads
//! the pad for the hotkeys, and the ring is drawn by the app as an overlay
//! (`overlay.rs`).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use romforge_core::Library;
use romforge_core::controller::PadEvent;
use romforge_core::hotkeys::Watcher;
use romforge_core::launch::{Ended, Playing, Session};
use romforge_core::uq::launcher::Hit;

use crate::overlay::{self, Overlay};

/// Why a game didn't start, said to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayError {
    /// Not a game in the library Play may start (bad input from the bus).
    Unknown,
    /// No library, a game already running, or the emulator wouldn't start.
    Failed(String),
}

/// How long a game gets to quit on its own before it is stopped hard.
const GRACE: Duration = Duration::from_secs(10);

pub struct Games {
    db: PathBuf,
    lib: Mutex<Option<Library>>,
    playing: Mutex<Option<Arc<Session>>>,
    /// The chords, and whether the hotkeys are on for the game running
    /// (Emulator Settings, read when it started).
    watcher: Watcher,
    hotkeys: AtomicBool,
    /// The pad is listened to from the first game with hotkeys on.
    listening: Mutex<bool>,
    /// The ring's overlay while such a game runs, and the app it is run
    /// from (found on first use when not set).
    overlay: Mutex<Option<Overlay>>,
    app: Mutex<Option<Option<PathBuf>>>,
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
            watcher: Watcher::new(),
            hotkeys: AtomicBool::new(false),
            listening: Mutex::new(false),
            overlay: Mutex::new(None),
            app: Mutex::new(None),
        }
    }

    /// Draw the ring with this program instead of the app it finds.
    pub fn with_app(self, program: Option<PathBuf>) -> Self {
        *lock(&self.app) = Some(program);
        self
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
        let (spec, hotkeys) = self
            .with(|lib| {
                if lib.launcher_game(id).map_err(|e| e.to_string())?.is_none() {
                    return Ok(None);
                }
                let env = lib.env().map_err(|e| e.to_string())?;
                let spec = lib.launch_row(&env, id, None).map_err(|e| e.to_string())?;
                Ok(Some((spec, lib.hotkeys_on().map_err(|e| e.to_string())?)))
            })
            .map_err(PlayError::Failed)?
            .ok_or(PlayError::Unknown)?;
        let session = Arc::new(Session::start(&spec).map_err(|e| PlayError::Failed(e.to_string()))?);
        let started = session.playing.clone();
        *playing = Some(session.clone());
        self.begin_hotkeys(hotkeys, &session);
        let games = self.clone();
        std::thread::spawn(move || {
            let end = session.wait();
            games.end_hotkeys(&session);
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

impl Games {
    /// The hotkeys for the game that just started, if they are on: the
    /// pad is listened to (once), and the ring's overlay started. Neither
    /// failing keeps the game from playing; they are said on stderr.
    fn begin_hotkeys(self: &Arc<Self>, on: bool, session: &Session) {
        self.hotkeys.store(on, Ordering::Relaxed);
        if !on {
            return;
        }
        let mut listening = lock(&self.listening);
        if !*listening {
            let games = self.clone();
            match romforge_core::controller::listen(move |e| games.pad(&e)) {
                Ok(()) => *listening = true,
                Err(e) => eprintln!("romforge-provider: no hotkeys, the pad can't be read: {e}"),
            }
        }
        drop(listening);
        let program = lock(&self.app).get_or_insert_with(overlay::app_program).clone();
        match program {
            Some(p) => match Overlay::start(&p, &session.playing.emulator) {
                Ok(o) => *lock(&self.overlay) = Some(o),
                Err(e) => eprintln!("romforge-provider: no radial menu, {}: {e}", p.display()),
            },
            None => eprintln!("romforge-provider: no radial menu, the romforge app wasn't found"),
        }
    }

    /// The game `ended` ended: the ring goes with it, unless a new game
    /// has been started since (the process can be gone, and Play accepted
    /// again, before this runs): its hotkeys and ring are not ours to end.
    /// Done under the lock of the game kept, which `play` holds while it
    /// starts the hotkeys of a new one.
    fn end_hotkeys(&self, ended: &Arc<Session>) {
        let playing = lock(&self.playing);
        if playing.as_ref().is_some_and(|s| !Arc::ptr_eq(s, ended)) {
            return;
        }
        self.hotkeys.store(false, Ordering::Relaxed);
        *lock(&self.overlay) = None;
    }

    /// What the pad did: the hotkeys, for the game this provider started
    /// while it runs; its ring's events go to the overlay.
    pub fn pad(&self, e: &PadEvent) {
        let game = lock(&self.playing)
            .clone()
            .filter(|s| s.is_running())
            .filter(|_| self.hotkeys.load(Ordering::Relaxed));
        let events = self.watcher.pad(game.as_ref(), e, GRACE);
        if events.is_empty() {
            return;
        }
        let mut overlay = lock(&self.overlay);
        if let Some(o) = overlay.as_mut() {
            for ev in &events {
                if o.send(ev).is_err() {
                    // The app is gone: no ring for the rest of this game.
                    *overlay = None;
                    break;
                }
            }
        }
    }
}
