//! Scanning folders and Refresh.

use romforge_core::{Refreshed, ScanOptions};

use crate::db::{Db, Res, read, with};
use crate::jobs::Job;

/// A job's scan options: its stop flag (threads from Settings).
fn options(job: &Job) -> ScanOptions {
    ScanOptions {
        cancel: job.cancel.clone(),
        ..ScanOptions::default()
    }
}

/// Scan folder: any folder (the first becomes the Source).
#[tauri::command]
pub async fn scan_folder(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    dir: String,
    job: Option<u64>,
) -> Res<romforge_core::FolderScan> {
    let mut job = Job::start(&app, job);
    with(&db, move |lib| {
        lib.scan_folder_with(&dir, &options(&job), |p| job.step(p.step()))
    })
    .await
}

/// Scan folder's dry run: what a scan would find; nothing is kept.
#[tauri::command]
pub async fn scan_preview(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    dir: String,
    job: Option<u64>,
) -> Res<romforge_core::ScanPreview> {
    let mut job = Job::start(&app, job);
    with(&db, move |lib| {
        lib.scan_preview_with(&dir, &options(&job), |p| job.step(p.step()))
    })
    .await
}

/// Whether Refresh has a folder to rescan.
#[tauri::command]
pub async fn can_refresh(db: tauri::State<'_, Db>) -> Res<bool> {
    read(&db, move |lib| lib.can_refresh()).await
}

#[tauri::command]
pub async fn refresh(app: tauri::AppHandle, db: tauri::State<'_, Db>, job: Option<u64>) -> Res<Refreshed> {
    // The toolbar's Refresh: the lists games are named by, then the
    // folders Settings names (not every folder ever scanned).
    let mut job = Job::start(&app, job);
    let (lists, results) = with(&db, move |lib| {
        let lists = lib.list_options(job.cancel.clone())?;
        lib.refresh_with_lists(false, &options(&job), &lists, |s| job.step(s))
    })
    .await?;
    Ok(Refreshed::of(lists, results))
}
