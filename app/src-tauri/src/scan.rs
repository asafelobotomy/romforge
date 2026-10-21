//! Scanning folders and Refresh.

use romforge_core::ScanOptions;

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

#[derive(serde::Serialize)]
pub struct Refreshed {
    folders: usize,
    entries: usize,
    problems: Vec<String>,
    /// Files skipped for their extension, by kind (`.txt 5, .nfo 3`).
    skipped: String,
}

#[tauri::command]
pub async fn refresh(app: tauri::AppHandle, db: tauri::State<'_, Db>, job: Option<u64>) -> Res<Refreshed> {
    // The toolbar's Refresh: the folders Settings names, not every folder
    // ever scanned.
    let mut job = Job::start(&app, job);
    let results = with(&db, move |lib| {
        lib.refresh_settings(&options(&job), |_, p| job.step(p.step()))
    })
    .await?;
    let mut out = Refreshed {
        folders: 0,
        entries: 0,
        problems: Vec::new(),
        skipped: String::new(),
    };
    let mut kinds = Vec::new();
    for r in results {
        match r {
            Ok(report) => {
                out.folders += 1;
                out.entries += report.hashed;
                kinds.push(report.skipped_kinds.clone());
                out.problems.extend(
                    report
                        .errors
                        .iter()
                        .map(|(p, m, e)| format!("{}#{m}: {e}", p.display())),
                );
            }
            Err((path, e)) => out.problems.push(format!("{}: {e}", path.display())),
        }
    }
    out.skipped = romforge_core::SkippedKind::summary(
        &romforge_core::SkippedKind::merge(kinds.iter().map(Vec::as_slice)),
        6,
    );
    Ok(out)
}
