//! Organize, cleanup, convert and the trash.

use std::path::{Path, PathBuf};

use romforge_core::Library;
use romforge_core::cleanup::{Batch as CleanupBatch, CleanupPlan, CleanupReport, CleanupUndone};
use romforge_core::organize::{Applied, OrganizeOptions, Plan, Undone};

use crate::db::{Db, Res, read, with};
use crate::jobs::Job;

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
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    from: String,
    dest: String,
    one_g1r: bool,
    job: Option<u64>,
) -> Res<Applied> {
    let mut job = Job::start(&app, job);
    with(&db, move |lib| {
        let plan = lib.organize_plan(&organize_options(&from, &dest, one_g1r))?;
        let cancel = job.cancel.clone();
        lib.organize_apply_with(&plan, &PathBuf::from(&dest), &cancel, |n, total| {
            job.count(n, total, format!("{n} of {total} files"))
        })
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
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    only: Option<Vec<String>>,
    from: Option<String>,
    job: Option<u64>,
) -> Res<CleanupReport> {
    let mut job = Job::start(&app, job);
    with(&db, move |lib| {
        let plan = plan_for(lib, only.as_deref(), from.as_deref())?;
        let (cancel, total) = (job.cancel.clone(), plan.items.len());
        job.count(0, total, "starting");
        lib.cleanup_apply_with(&plan, &cancel, |n, title| job.count(n + 1, total, title))
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
pub async fn convert_apply(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    only: Option<Vec<String>>,
    job: Option<u64>,
) -> Res<CleanupReport> {
    let mut job = Job::start(&app, job);
    with(&db, move |lib| {
        let cancel = job.cancel.clone();
        lib.convert_apply_with(only.as_deref(), &cancel, |n, total, title| {
            job.count(n + 1, total, title)
        })
    })
    .await
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
