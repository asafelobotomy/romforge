//! Organize, cleanup, convert and the trash.

use std::path::{Path, PathBuf};

use romforge_core::Library;
use romforge_core::cleanup::{Batch as CleanupBatch, CleanupPlan, CleanupReport, CleanupUndone};
use romforge_core::organize::{Applied, OrganizeOptions, Plan, Undone};

use crate::db::{Db, Res, read, with};

fn organize_options(from: &str, dest: &str, one_g1r: bool) -> OrganizeOptions {
    OrganizeOptions {
        one_g1r,
        ..OrganizeOptions::new(from, dest)
    }
}

#[tauri::command]
pub async fn organize_plan(db: tauri::State<'_, Db>, from: String, dest: String, one_g1r: bool) -> Res<Plan> {
    read(&db, move |lib| {
        lib.organize_plan(&organize_options(&from, &dest, one_g1r))
    })
    .await
}

#[tauri::command]
pub async fn organize_apply(
    db: tauri::State<'_, Db>,
    from: String,
    dest: String,
    one_g1r: bool,
) -> Res<Applied> {
    with(&db, move |lib| {
        let plan = lib.organize_plan(&organize_options(&from, &dest, one_g1r))?;
        lib.organize_apply(&plan, &PathBuf::from(&dest))
    })
    .await
}

#[tauri::command]
pub async fn organize_undo(db: tauri::State<'_, Db>) -> Res<Undone> {
    with(&db, move |lib| lib.organize_undo(None)).await
}

/// `from`: the import plan for that folder (milestone 13); else `only`:
/// these library row ids, else every identified game in Source (or, with
/// a RomForge folder, everywhere outside it).
fn plan_for(
    lib: &Library,
    only: Option<&[String]>,
    from: Option<&str>,
) -> Result<CleanupPlan, romforge_core::Error> {
    match from {
        Some(dir) => lib.import_plan(Some(Path::new(dir))),
        None => lib.cleanup_plan(only),
    }
}

#[tauri::command]
pub async fn cleanup_plan(
    db: tauri::State<'_, Db>,
    only: Option<Vec<String>>,
    from: Option<String>,
) -> Res<CleanupPlan> {
    read(&db, move |lib| plan_for(lib, only.as_deref(), from.as_deref())).await
}

/// Plans again here rather than trusting a plan sent back from the window.
#[tauri::command]
pub async fn cleanup_apply(
    db: tauri::State<'_, Db>,
    only: Option<Vec<String>>,
    from: Option<String>,
) -> Res<CleanupReport> {
    with(&db, move |lib| {
        let plan = plan_for(lib, only.as_deref(), from.as_deref())?;
        lib.cleanup_apply(&plan, |_, _| {})
    })
    .await
}

/// Convert's plan: disc images into CHD where they are.
#[tauri::command]
pub async fn convert_plan(db: tauri::State<'_, Db>, only: Option<Vec<String>>) -> Res<CleanupPlan> {
    read(&db, move |lib| lib.convert_plan(only.as_deref())).await
}

/// Convert (planned again here).
#[tauri::command]
pub async fn convert_apply(db: tauri::State<'_, Db>, only: Option<Vec<String>>) -> Res<CleanupReport> {
    with(&db, move |lib| lib.convert_apply(only.as_deref(), |_, _| {})).await
}

#[tauri::command]
pub async fn cleanup_undo(db: tauri::State<'_, Db>) -> Res<CleanupUndone> {
    with(&db, move |lib| lib.cleanup_undo(None)).await
}

#[tauri::command]
pub async fn trash_contents(db: tauri::State<'_, Db>) -> Res<romforge_core::cleanup::TrashSummary> {
    read(&db, move |lib| lib.trash_contents(None)).await
}

#[tauri::command]
pub async fn empty_trash(db: tauri::State<'_, Db>) -> Res<romforge_core::cleanup::TrashSummary> {
    with(&db, move |lib| lib.empty_trash(None)).await
}

#[tauri::command]
pub async fn cleanup_batches(db: tauri::State<'_, Db>) -> Res<Vec<CleanupBatch>> {
    read(&db, move |lib| lib.cleanup_batches()).await
}
