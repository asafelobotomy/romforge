//! Playing a game (milestone 18, 1.6): one at a time, tracked while it
//! runs, stopped by asking it to quit first, its play time recorded when
//! it ends ("game-ended" tells the page).

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
    let write = db.write.clone();
    std::thread::spawn(move || {
        let ended = session.wait();
        {
            let mut lib = write.lock().unwrap_or_else(|e| e.into_inner());
            let _ = lib.record_play(&ended);
        }
        crate::tray::game_ended(&app);
        let _ = app.emit("game-ended", &ended);
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
