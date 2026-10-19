//! Settings.

use romforge_core::settings::{Settings, default_staging_dir};

use crate::db::{Db, Res, read, with};

#[tauri::command]
pub async fn get_settings(db: tauri::State<'_, Db>) -> Res<Settings> {
    read(&db, move |lib| lib.settings_masked()).await
}

#[tauri::command]
pub async fn save_settings(db: tauri::State<'_, Db>, settings: Settings) -> Res<Settings> {
    with(&db, move |lib| lib.save_settings_from_form(&settings)).await
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
