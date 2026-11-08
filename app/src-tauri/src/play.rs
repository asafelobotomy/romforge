//! Playing a game (milestone 18, 1.6): one at a time, tracked while it
//! runs, stopped by asking it to quit first, its play time recorded when
//! it ends ("game-ended" tells the page).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use romforge_core::launch::{Playing, Session};
use tauri::Emitter;

use crate::db::{Db, Res, blocking, read};

/// The game being played, if one is.
#[derive(Default)]
pub struct Now(pub Mutex<Option<Arc<Session>>>);

/// How long a game gets to quit on its own before it is stopped hard.
pub(crate) const GRACE: Duration = Duration::from_secs(10);

/// A game is being started (between the check and the session being
/// kept): a second Play meanwhile is refused, not started beside it.
static STARTING: AtomicBool = AtomicBool::new(false);

/// romforge was asked to quit while a game ran: it goes once the game has
/// ended and its play time is kept.
static QUIT_AFTER: AtomicBool = AtomicBool::new(false);

/// Whether a game is running now.
pub fn game_running(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;
    let now = app.state::<Now>();
    let s = now.0.lock().unwrap_or_else(|e| e.into_inner()).clone();
    s.is_some_and(|s| s.is_running())
}

/// The window was closed while a game plays: stop it and quit, or keep
/// playing (the window stays).
pub fn ask_to_quit(app: &tauri::AppHandle) {
    use tauri::Manager;
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let game = app
        .state::<Now>()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|s| s.playing.game.clone())
        .unwrap_or_default();
    let handle = app.clone();
    app.dialog()
        .message(format!(
            "{game} is still running. Stop it (it is asked to quit, so it saves) and quit romforge?"
        ))
        .title("Quit romforge")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Stop and quit".into(),
            "Keep playing".into(),
        ))
        .show(move |yes| {
            if yes {
                quit_after_game(&handle);
            }
        });
}

/// Quit romforge: at once with no game, else after stopping the game (it
/// is asked to quit first, so it saves) and keeping its play time.
pub fn quit_after_game(app: &tauri::AppHandle) {
    use tauri::Manager;
    let now = app.state::<Now>();
    let session = now.0.lock().unwrap_or_else(|e| e.into_inner()).clone();
    match session.filter(|s| s.is_running()) {
        Some(s) => {
            QUIT_AFTER.store(true, Ordering::SeqCst);
            std::thread::spawn(move || s.stop(GRACE));
        }
        None => app.exit(0),
    }
}

struct Starting;
impl Drop for Starting {
    fn drop(&mut self) {
        STARTING.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
/// Start one library row (exactly its files).
pub async fn launch(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    now: tauri::State<'_, Now>,
    row_id: String,
) -> Res<Playing> {
    if let Some(s) = now.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        && s.is_running()
    {
        return Err(format!("{} is still running: stop it first", s.playing.game));
    }
    if STARTING.swap(true, Ordering::SeqCst) {
        return Err("a game is starting: wait for it".into());
    }
    let _starting = Starting;
    let (spec, hotkeys) = read(&db, move |lib| {
        Ok((lib.launch_row(&lib.env()?, &row_id, None)?, lib.hotkeys_on()?))
    })
    .await?;
    crate::hotkeys::set_on(&app, hotkeys);
    if hotkeys {
        crate::overlay::prepare(&app);
    }
    let session = Arc::new(blocking(move || Session::start(&spec)).await?);
    let playing = session.playing.clone();
    *now.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(session.clone());
    let (write, read, detached) = (db.write.clone(), db.read.clone(), db.detached.clone());
    std::thread::spawn(move || {
        let ended = session.wait();
        let quitting = QUIT_AFTER.load(Ordering::SeqCst);
        // The window back first: nothing the database is busy with keeps
        // it away.
        if !quitting {
            crate::tray::game_ended(&app);
        }
        // Play time on a connection of its own, so a long job holding
        // `write` (Enrich, a scan) doesn't wait on it. If the database is
        // busy that long (a scan), it is kept once `write` is free instead.
        let quick = crate::db::apart(&read, &detached, |at| {
            romforge_core::Library::edit(at, |lib| lib.record_play(&ended).map(drop))
        });
        let _ = app.emit("game-ended", &ended);
        if let Err(first) = quick {
            let mut lib = write.lock().unwrap_or_else(|e| e.into_inner());
            if let Err(e) = lib.record_play(&ended) {
                eprintln!(
                    "romforge: {}'s play time wasn't kept ({first}; then {e})",
                    ended.playing.game
                );
            }
        }
        if quitting {
            app.exit(0);
        }
    });
    Ok(playing)
}

/// Ask the running game to quit (it saves), and stop it only if it hasn't
/// after a few seconds. Returns whether it had to be stopped hard.
#[tauri::command]
pub async fn stop_game(now: tauri::State<'_, Now>) -> Res<bool> {
    let Some(s) = now.0.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
        return Ok(false);
    };
    blocking(move || s.stop(GRACE)).await
}

#[tauri::command]
pub fn now_playing(now: tauri::State<'_, Now>) -> Option<Playing> {
    now.0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .filter(|s| s.is_running())
        .map(|s| s.playing.clone())
}
