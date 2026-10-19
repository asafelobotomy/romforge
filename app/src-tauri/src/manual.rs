//! A game's manual (milestone 26).

use std::path::PathBuf;

use romforge_core::manual::ManualInfo;

use crate::db::{Db, Res, read, read_net, with};

#[tauri::command]
pub async fn manual_info(db: tauri::State<'_, Db>, row_id: String) -> Res<ManualInfo> {
    read(&db, move |lib| lib.manual_info(&row_id)).await
}

/// Ask Hasheous (once per game) for its Vimm's Lair manual id.
#[tauri::command]
pub async fn manual_lookup(db: tauri::State<'_, Db>, row_id: String) -> Res<Option<String>> {
    read_net(&db, move |lib| lib.manual_lookup(&row_id)).await
}

#[tauri::command]
pub async fn add_manual(db: tauri::State<'_, Db>, row_id: String, path: PathBuf) -> Res<PathBuf> {
    with(&db, move |lib| lib.add_manual(&row_id, &path)).await
}

/// Download it from Vimm's Lair: only ever on the user's click.
#[tauri::command]
pub async fn download_manual(db: tauri::State<'_, Db>, row_id: String, dpi: u32) -> Res<PathBuf> {
    with(&db, move |lib| lib.download_manual(&row_id, dpi)).await
}

#[tauri::command]
pub async fn remove_manual(db: tauri::State<'_, Db>, row_id: String) -> Res<()> {
    with(&db, move |lib| lib.remove_manual(&row_id)).await
}

#[tauri::command]
pub async fn set_manual_page(db: tauri::State<'_, Db>, row_id: String, page: u32) -> Res<()> {
    with(&db, move |lib| lib.set_manual_page(&row_id, page)).await
}
