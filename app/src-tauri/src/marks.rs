//! The fullscreen mode's extras (stage 2, step 6): favourites, play time
//! for Recently played and Most played, and the emulator picked per game.

use romforge_core::user_games::Marks;

use crate::db::{Db, Res, edit, read};

/// Favourites and every game's play time.
#[tauri::command]
pub async fn marks(db: tauri::State<'_, Db>) -> Res<Marks> {
    read(&db, |lib| lib.marks()).await
}

#[tauri::command]
pub async fn set_favourite(db: tauri::State<'_, Db>, system: String, title: String, on: bool) -> Res<()> {
    edit(&db, move |lib| lib.set_favourite(&system, &title, on)).await
}

#[derive(serde::Serialize)]
pub struct GameEmulator {
    /// The one picked for the game (none: romforge's choice).
    picked: Option<String>,
    /// The installed ones that play its system, in romforge's order.
    choices: Vec<String>,
}

#[tauri::command]
pub async fn game_emulator(db: tauri::State<'_, Db>, system: String, title: String) -> Res<GameEmulator> {
    read(&db, move |lib| {
        let (picked, choices) = lib.game_emulator(&lib.env()?, &system, &title)?;
        Ok(GameEmulator { picked, choices })
    })
    .await
}

#[tauri::command]
pub async fn set_game_emulator(
    db: tauri::State<'_, Db>,
    system: String,
    title: String,
    name: Option<String>,
) -> Res<()> {
    edit(&db, move |lib| {
        let env = lib.env()?;
        lib.set_game_emulator(&env, &system, &title, name.as_deref())
    })
    .await
}

/// What Play does with a row: start one now (`run`), or offer the game's
/// versions to pick from.
#[tauri::command]
pub async fn play_choice(
    db: tauri::State<'_, Db>,
    row_id: String,
) -> Res<romforge_core::user_games::versions::PlayChoice> {
    read(&db, move |lib| lib.play_choice(&row_id)).await
}

/// The version Play always runs for a row's game, if one is kept.
#[tauri::command]
pub async fn always_run(
    db: tauri::State<'_, Db>,
    row_id: String,
) -> Res<Option<romforge_core::view::GameRow>> {
    read(&db, move |lib| lib.always_run(&row_id)).await
}

/// Keep a version as the one Play always runs (none: ask again).
#[tauri::command]
pub async fn set_always_run(db: tauri::State<'_, Db>, row_id: String, version: Option<String>) -> Res<()> {
    edit(&db, move |lib| lib.set_always_run(&row_id, version.as_deref())).await
}
