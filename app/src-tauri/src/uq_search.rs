//! Settings → uQ search: romforge's launcher provider as the user's own
//! files (romforge_core::uq::user_provider), only when the user asks.

use romforge_core::uq::user_provider::{self as up, Status};

use crate::db::{Res, blocking};

#[tauri::command]
pub async fn uq_search_status() -> Res<Option<Status>> {
    blocking(|| Ok(up::status())).await
}

#[tauri::command]
pub async fn uq_search_add() -> Res<()> {
    blocking(up::add).await
}

#[tauri::command]
pub async fn uq_search_remove() -> Res<bool> {
    blocking(up::remove).await
}
