//! The BIOS page (milestone 20).

use std::path::{Path, PathBuf};

use romforge_core::firmware::{FirmwareReport, ImportPlan, Imported};

use crate::db::{Db, Res, read, read_net, with};

#[tauri::command]
pub async fn firmware_report(db: tauri::State<'_, Db>) -> Res<FirmwareReport> {
    read(&db, move |lib| lib.firmware_report()).await
}

/// The usual BIOS places that exist on this computer.
#[tauri::command]
pub async fn firmware_found(db: tauri::State<'_, Db>) -> Res<Vec<PathBuf>> {
    read(&db, move |lib| lib.firmware_found()).await
}

/// The scanned files that are BIOS files (all, or one file's).
#[tauri::command]
pub async fn firmware_in_library(db: tauri::State<'_, Db>, file_id: Option<i64>) -> Res<Vec<PathBuf>> {
    read(&db, move |lib| lib.firmware_in_library(file_id)).await
}

/// The library's BIOS badges.
#[tauri::command]
pub async fn firmware_needs(db: tauri::State<'_, Db>) -> Res<Vec<romforge_core::firmware::BiosNeed>> {
    read(&db, move |lib| Ok(lib.firmware_needs())).await
}

/// Fetch RetroBIOS's list (the list only, never BIOS files).
#[tauri::command]
pub async fn firmware_download(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    job: Option<u64>,
) -> Res<usize> {
    let job = crate::jobs::Job::start(&app, job);
    read_net(&db, move |lib| {
        job.watch("RetroBIOS list", || lib.firmware_download())
    })
    .await
}

/// A file's name, as a job's step.
fn named(p: &Path) -> String {
    p.file_name()
        .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// What importing dropped files would do (a job: each file read is a step).
#[tauri::command]
pub async fn firmware_import_plan(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    paths: Vec<PathBuf>,
    job: Option<u64>,
) -> Res<ImportPlan> {
    let mut job = crate::jobs::Job::start(&app, job);
    read(&db, move |lib| {
        lib.firmware_import_plan(&paths, |n, all, f| job.count(n, all, named(f)))
    })
    .await
}

/// Import them (a job: every file read again, then copied).
#[tauri::command]
pub async fn firmware_import(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    paths: Vec<PathBuf>,
    also: Vec<String>,
    job: Option<u64>,
) -> Res<Imported> {
    let mut job = crate::jobs::Job::start(&app, job);
    with(&db, move |lib| {
        lib.firmware_import(&paths, &also, |n, all, f| job.count(n, all, named(f)))
    })
    .await
}

#[tauri::command]
pub async fn firmware_undo(db: tauri::State<'_, Db>) -> Res<usize> {
    with(&db, move |lib| lib.firmware_undo()).await
}

/// Install the PS3 firmware from Bios/rpcs3/ into RPCS3.
#[tauri::command]
pub async fn install_ps3_firmware(db: tauri::State<'_, Db>) -> Res<String> {
    read_net(&db, move |lib| {
        lib.install_ps3_firmware().map(|p| p.display().to_string())
    })
    .await
}
