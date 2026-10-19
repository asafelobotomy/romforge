//! Emulator Settings.

use romforge_core::emusettings::{EmulatorSettings, EmulatorSettingsView};

use crate::db::{Db, Res, read, with};

#[tauri::command]
pub async fn get_emulator_settings(db: tauri::State<'_, Db>) -> Res<EmulatorSettingsView> {
    read(&db, move |lib| lib.emulator_settings()).await
}

#[tauri::command]
pub async fn save_emulator_settings(
    db: tauri::State<'_, Db>,
    settings: EmulatorSettings,
) -> Res<EmulatorSettings> {
    with(&db, move |lib| lib.save_emulator_settings(&settings)).await
}

#[tauri::command]
pub async fn import_emulator_setup(
    db: tauri::State<'_, Db>,
    emulator: String,
) -> Res<romforge_core::launch::existing::SetupImported> {
    with(&db, move |lib| lib.import_emulator_setup(&emulator)).await
}

#[tauri::command]
pub async fn undo_emulator_setup(db: tauri::State<'_, Db>, emulator: String) -> Res<usize> {
    with(&db, move |lib| lib.undo_emulator_setup(&emulator)).await
}

#[tauri::command]
pub async fn emulator_cores(db: tauri::State<'_, Db>, refresh: bool) -> Res<romforge_core::obtain::CoreList> {
    read(&db, move |lib| {
        if refresh {
            lib.refresh_emulator_cores()?;
        }
        lib.emulator_cores()
    })
    .await
}

#[tauri::command]
pub async fn download_core(db: tauri::State<'_, Db>, core: String) -> Res<String> {
    read(&db, move |lib| lib.download_core(&core)).await
}

#[tauri::command]
pub async fn remove_core(db: tauri::State<'_, Db>, core: String) -> Res<()> {
    read(&db, move |lib| lib.remove_core(&core)).await
}

#[tauri::command]
pub async fn install_emulator(db: tauri::State<'_, Db>, emulator: String) -> Res<String> {
    read(&db, move |lib| lib.install_emulator(&emulator)).await
}
