//! The Saves page and the saves line in a game's Details (milestone 27).

use romforge_core::saves::page::{GameSaves, SystemSaves};

use crate::db::{Db, Res, read};

/// Every save in `Saves/`, one tab per system.
#[tauri::command]
pub async fn saves_page(db: tauri::State<'_, Db>) -> Res<Vec<SystemSaves>> {
    read(&db, move |lib| lib.saves_page()).await
}

/// How many saves a library row's game has, and the newest one's time.
#[tauri::command]
pub async fn game_saves(db: tauri::State<'_, Db>, id: String) -> Res<GameSaves> {
    read(&db, move |lib| lib.game_saves(&id)).await
}
