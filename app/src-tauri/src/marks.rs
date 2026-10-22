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
