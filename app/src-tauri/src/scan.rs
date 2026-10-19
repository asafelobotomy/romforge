//! Scanning folders and Refresh.

use romforge_core::{ScanReport, Scope};

use crate::db::{Db, Res, read, with};

#[tauri::command]
pub async fn scan(db: tauri::State<'_, Db>, dir: String) -> Res<ScanReport> {
    with(&db, move |lib| lib.scan(&dir, Scope::All)).await
}

/// Scan folder: any folder (the first becomes the Source).
#[tauri::command]
pub async fn scan_folder(db: tauri::State<'_, Db>, dir: String) -> Res<romforge_core::FolderScan> {
    with(&db, move |lib| lib.scan_folder(&dir)).await
}

/// Scan folder's dry run: what a scan would find; nothing is kept.
#[tauri::command]
pub async fn scan_preview(db: tauri::State<'_, Db>, dir: String) -> Res<romforge_core::ScanPreview> {
    with(&db, move |lib| lib.scan_preview(&dir)).await
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
pub async fn refresh(db: tauri::State<'_, Db>) -> Res<Refreshed> {
    // The toolbar's Refresh: the folders Settings names, not every folder
    // ever scanned.
    let results = with(&db, move |lib| {
        lib.refresh_settings(Default::default(), |_, _| {})
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
