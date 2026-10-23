//! Settings.

use romforge_core::settings::{Settings, default_staging_dir};

use crate::db::{Db, Res, edit, read};

#[tauri::command]
pub async fn get_settings(db: tauri::State<'_, Db>) -> Res<Settings> {
    read(&db, move |lib| lib.settings_masked()).await
}

#[tauri::command]
pub async fn save_settings(db: tauri::State<'_, Db>, settings: Settings) -> Res<Settings> {
    edit(&db, move |lib| lib.save_settings_from_form(&settings)).await
}

/// The fullscreen mode's Start → Details: keep which details it shows.
#[tauri::command]
pub async fn set_fs_details(db: tauri::State<'_, Db>, shown: Vec<String>) -> Res<Vec<String>> {
    edit(&db, move |lib| lib.set_fs_details(&shown)).await
}

/// The fullscreen mode's Start → Bookshelf: keep how it stands the games.
#[tauri::command]
pub async fn set_fs_bookshelf(
    db: tauri::State<'_, Db>,
    shelf: romforge_core::settings::Bookshelf,
) -> Res<romforge_core::settings::Bookshelf> {
    edit(&db, move |lib| lib.set_fs_bookshelf(&shelf)).await
}

#[derive(serde::Serialize)]
pub struct SettingsInfo {
    default_staging: String,
}

#[tauri::command]
pub async fn settings_info() -> Res<SettingsInfo> {
    Ok(SettingsInfo {
        default_staging: default_staging_dir().display().to_string(),
    })
}
