//! The Saves page, the saves line in a game's Details, and moving the old
//! `States/` into `Saves/` (milestone 27).

use romforge_core::saves::gather::GatherPlan;
use romforge_core::saves::page::{GameSaves, SystemSaves};

use crate::db::{Db, Res, read, with};

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

/// What moving the save states left in the old `States/` into `Saves/`
/// would do.
#[tauri::command]
pub async fn merge_states_plan(db: tauri::State<'_, Db>) -> Res<GatherPlan> {
    read(&db, move |lib| lib.merge_states_plan()).await
}

/// Move them, journaled: (journal id, files moved).
#[tauri::command]
pub async fn merge_states(db: tauri::State<'_, Db>) -> Res<(i64, usize)> {
    with(&db, move |lib| lib.merge_states()).await
}

/// Put the last move back into `States/`: how many files went back.
#[tauri::command]
pub async fn undo_merge_states(db: tauri::State<'_, Db>) -> Res<usize> {
    with(&db, move |lib| lib.undo_merge_states()).await
}
