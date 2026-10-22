//! Search: identifying by hand and settling Wikidata matches.

use romforge_core::enrich::{EnrichReport, default_cache_dir};
use romforge_core::search::{Chosen, Found, Preview, QueueItem, Scraper};

use crate::db::{Db, Res, edit, read, read_net, with, write_net};

#[tauri::command]
pub async fn search_queue(db: tauri::State<'_, Db>, only: Option<Vec<String>>) -> Res<Vec<QueueItem>> {
    read(&db, move |lib| lib.search_queue(only.as_deref())).await
}

/// Name rows by their strongest suggestion (Search's "Name as suggested").
#[tauri::command]
pub async fn search_accept(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    only: Vec<String>,
    job: Option<u64>,
) -> Res<Vec<romforge_core::search::Accepted>> {
    let mut job = crate::jobs::Job::start(&app, job);
    with(&db, move |lib| {
        let cancel = job.cancel.clone();
        lib.search_accept_with(&only, &cancel, |n, total| {
            job.count(n, total, format!("{n} of {total} games"))
        })
    })
    .await
}

#[tauri::command]
pub async fn forget_identity(db: tauri::State<'_, Db>, row_id: String) -> Res<()> {
    edit(&db, move |lib| lib.forget_identity(&row_id)).await
}

#[tauri::command]
pub async fn search_find(
    db: tauri::State<'_, Db>,
    row_id: String,
    scraper: Scraper,
    text: Option<String>,
    system: Option<String>,
) -> Res<Vec<Found>> {
    read_net(&db, move |lib| {
        lib.search_find(
            &row_id,
            scraper,
            text.as_deref(),
            system.as_deref(),
            romforge_core::search::WIKIDATA_API,
        )
    })
    .await
}

/// The preview; its picture (`image`) stays a file path in the cache,
/// shown through `art://`.
#[tauri::command]
pub async fn search_preview(db: tauri::State<'_, Db>, row_id: String, found: Found) -> Res<Preview> {
    read_net(&db, move |lib| {
        lib.search_preview(
            &row_id,
            &found,
            &default_cache_dir(),
            romforge_core::enrich::THUMBNAILS_BASE,
        )
    })
    .await
}

/// Name or settle a Search result (no art fetch). The dialog advances the
/// queue on this; Enrich for the chips runs afterwards.
#[tauri::command]
pub async fn search_apply(
    db: tauri::State<'_, Db>,
    row_id: String,
    found: Found,
    system: Option<String>,
) -> Res<Chosen> {
    edit(&db, move |lib| {
        lib.search_apply(&row_id, &found, system.as_deref())
    })
    .await
}

/// Search's fetch after a pick, for the row [`search_apply`] gave: on its
/// own connection ([`write_net`]), so the next pick doesn't wait on it.
#[tauri::command]
pub async fn search_fetch(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    row_id: String,
    art: Vec<String>,
    wikidata: bool,
    overwrite: bool,
    job: Option<u64>,
) -> Res<EnrichReport> {
    let mut job = crate::jobs::Job::start(&app, job);
    let mut opts = crate::enrich::options(None, Some(art), Some(wikidata), Some(overwrite))?;
    opts.cancel = job.cancel.clone();
    write_net(&db, move |lib| {
        lib.enrich_rows(&opts, &[row_id], |e| crate::enrich::report(&mut job, e))
    })
    .await
}
