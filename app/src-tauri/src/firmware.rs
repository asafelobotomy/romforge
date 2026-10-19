//! The BIOS page (milestone 20).

use std::path::PathBuf;

use romforge_core::firmware::{FirmwareReport, ImportPlan, Imported};

use crate::db::{Db, Res, read, with};

#[tauri::command]
pub async fn firmware_report(db: tauri::State<'_, Db>) -> Res<FirmwareReport> {
    read(&db, move |lib| lib.firmware_report()).await
}

/// Fetch RetroBIOS's list (the list only, never BIOS files).
#[tauri::command]
pub async fn firmware_download(db: tauri::State<'_, Db>) -> Res<usize> {
    read(&db, move |lib| lib.firmware_download()).await
}

/// What importing dropped files would do.
#[tauri::command]
pub async fn firmware_import_plan(db: tauri::State<'_, Db>, paths: Vec<PathBuf>) -> Res<ImportPlan> {
    read(&db, move |lib| lib.firmware_import_plan(&paths)).await
}

#[tauri::command]
pub async fn firmware_import(
    db: tauri::State<'_, Db>,
    paths: Vec<PathBuf>,
    also: Vec<String>,
) -> Res<Imported> {
    with(&db, move |lib| lib.firmware_import(&paths, &also)).await
}

#[tauri::command]
pub async fn firmware_undo(db: tauri::State<'_, Db>) -> Res<usize> {
    with(&db, move |lib| lib.firmware_undo()).await
}

/// Install the PS3 firmware from Bios/rpcs3/ into RPCS3.
#[tauri::command]
pub async fn install_ps3_firmware(db: tauri::State<'_, Db>) -> Res<String> {
    read(&db, move |lib| {
        lib.install_ps3_firmware().map(|p| p.display().to_string())
    })
    .await
}
