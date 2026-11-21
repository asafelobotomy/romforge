//! Scanning folders and Refresh.

use romforge_core::{ListsReport, Refreshed, ScanOptions, Scope};

use crate::db::{Db, Res, read, with};
use crate::jobs::Job;

/// What the Advanced part of Scan folder and Refresh asks for: what the
/// CLI's `scan` and `refresh` take as flags. All off or empty is what the
/// plain buttons do.
#[derive(Default, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RunOptions {
    /// Hashing threads; 0: Settings' threads.
    jobs: usize,
    /// Hash every file again, even unchanged ones.
    rehash: bool,
    /// Hash every file, not only known ROM extensions.
    all_files: bool,
    /// Scan folder: identify against one platform (its key)...
    system: Option<String>,
    /// ...or one DAT (its id); neither: the scope the folder had, else all.
    dat: Option<i64>,
    /// Refresh: leave the online lists alone.
    no_lists: bool,
    /// Refresh: every folder ever scanned, not only Settings' (and the
    /// RomForge folder).
    every_folder: bool,
}

impl RunOptions {
    /// The scan's options: the job's stop flag and these (threads from
    /// Settings when `jobs` is 0).
    fn scan(&self, job: &Job) -> ScanOptions {
        ScanOptions {
            all_files: self.all_files,
            jobs: self.jobs,
            rehash: self.rehash,
            cancel: job.cancel.clone(),
        }
    }

    /// The scope asked for; none: let the folder keep its own.
    fn scope(&self) -> Result<Option<Scope>, romforge_core::Error> {
        if self.system.is_none() && self.dat.is_none() {
            return Ok(None);
        }
        Scope::choose(self.system.as_deref(), self.dat).map(Some)
    }
}

/// Scan folder: any folder (the first becomes the Source), with ScummVM's
/// list brought up to date around it so its games are one row each.
#[tauri::command]
pub async fn scan_folder(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    dir: String,
    options: Option<RunOptions>,
    job: Option<u64>,
) -> Res<romforge_core::FolderScan> {
    let mut job = Job::start(&app, job);
    let o = options.unwrap_or_default();
    with(&db, move |lib| {
        lib.scan_folder_with_scummvm(&dir, o.scope()?, &o.scan(&job), None, |p| job.step(p.step()))
    })
    .await
}

/// Scan folder's dry run: what a scan would find; nothing is kept.
#[tauri::command]
pub async fn scan_preview(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    dir: String,
    options: Option<RunOptions>,
    job: Option<u64>,
) -> Res<romforge_core::ScanPreview> {
    let mut job = Job::start(&app, job);
    let o = options.unwrap_or_default();
    with(&db, move |lib| {
        lib.scan_preview_scoped(&dir, o.scope()?, &o.scan(&job), |p| job.step(p.step()))
    })
    .await
}

/// Whether Refresh has a folder to rescan.
#[tauri::command]
pub async fn can_refresh(db: tauri::State<'_, Db>) -> Res<bool> {
    read(&db, move |lib| lib.can_refresh()).await
}

/// The toolbar's Refresh: the lists games are named by, then the folders
/// Settings names (not every folder ever scanned), as `options` changes it.
#[tauri::command]
pub async fn refresh(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    options: Option<RunOptions>,
    job: Option<u64>,
) -> Res<Refreshed> {
    let mut job = Job::start(&app, job);
    let o = options.unwrap_or_default();
    let (lists, results) = with(&db, move |lib| {
        let scan = o.scan(&job);
        if o.no_lists {
            let results = if o.every_folder {
                lib.refresh(&scan, |_, p| job.step(p.step()))?
            } else {
                lib.refresh_settings(&scan, |_, p| job.step(p.step()))?
            };
            return Ok((ListsReport::default(), results));
        }
        let lists = lib.list_options(job.cancel.clone())?;
        lib.refresh_with_lists(o.every_folder, &scan, &lists, |s| job.step(s))
    })
    .await?;
    Ok(Refreshed::of(lists, results))
}
