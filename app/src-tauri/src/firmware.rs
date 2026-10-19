//! The BIOS page (milestone 20).

use romforge_core::firmware::FirmwareReport;

use crate::db::{Db, Res, read};

#[tauri::command]
pub async fn firmware_report(db: tauri::State<'_, Db>) -> Res<FirmwareReport> {
    read(&db, move |lib| lib.firmware_report()).await
}

/// Fetch RetroBIOS's list (the list only, never BIOS files).
#[tauri::command]
pub async fn firmware_download(db: tauri::State<'_, Db>) -> Res<usize> {
    read(&db, move |lib| lib.firmware_download()).await
}
