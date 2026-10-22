//! Enrich, the art picker and the platform icons.

use romforge_core::enrich::{ArtKind, EnrichOptions, EnrichReport, default_cache_dir};

use crate::db::{Db, Res, edit, read_net, with};
use crate::jobs::Job;

/// Send an enrich run's progress for its job.
pub fn report(job: &mut Job, e: romforge_core::enrich::EnrichEvent) {
    if let romforge_core::enrich::EnrichEvent::Progress(s) = e {
        job.step(s);
    }
}

/// Enrich's options from the app's chips: `art` kinds (`None`: every
/// kind), `wikidata` (`None`: yes), `overwrite` to fetch again.
pub fn options(
    system: Option<String>,
    art: Option<Vec<String>>,
    wikidata: Option<bool>,
    overwrite: Option<bool>,
) -> Res<EnrichOptions> {
    EnrichOptions::chosen(
        system,
        art.as_deref(),
        wikidata.unwrap_or(true),
        overwrite.unwrap_or(false),
    )
    .map_err(|e| e.to_string())
}

/// The sidebar's platform icons (fetched once from retroarch-assets):
/// system key -> file, shown through `art://`.
#[tauri::command]
pub async fn platform_icons(db: tauri::State<'_, Db>) -> Res<romforge_core::PlatformIcons> {
    read_net(&db, move |lib| {
        lib.platform_icons(
            &default_cache_dir(),
            romforge_core::enrich::PLATFORM_ICONS_BASE,
            romforge_core::wikidata::ENDPOINT,
        )
    })
    .await
}

/// One platform's logo (Wikidata, cached), for the splash.
#[tauri::command]
pub async fn platform_logo(db: tauri::State<'_, Db>, system: String) -> Res<Option<std::path::PathBuf>> {
    read_net(&db, move |lib| {
        lib.platform_logo(
            &default_cache_dir(),
            romforge_core::enrich::PLATFORM_ICONS_BASE,
            romforge_core::wikidata::ENDPOINT,
            &system,
        )
    })
    .await
}

/// `only`: these library rows (the selection), else every game of
/// `system` (or of every platform); the rest as in [`options`].
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn enrich(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    system: Option<String>,
    only: Option<Vec<String>>,
    art: Option<Vec<String>>,
    wikidata: Option<bool>,
    overwrite: Option<bool>,
    job: Option<u64>,
) -> Res<EnrichReport> {
    let mut job = Job::start(&app, job);
    let mut opts = options(system, art, wikidata, overwrite)?;
    opts.cancel = job.cancel.clone();
    with(&db, move |lib| match &only {
        Some(ids) => lib.enrich_rows(&opts, ids, |e| report(&mut job, e)),
        None => lib.enrich(&opts, |e| report(&mut job, e)),
    })
    .await
}

/// The pictures of `kind` the sources have for a row (Details' art
/// picker); previews stay file paths, shown through `art://`.
#[tauri::command]
pub async fn art_choices(
    db: tauri::State<'_, Db>,
    row_id: String,
    kind: String,
) -> Res<romforge_core::enrich::ArtChoices> {
    let kind = ArtKind::parse(&kind).ok_or_else(|| format!("no art kind {kind:?}"))?;
    let opts = EnrichOptions::new(default_cache_dir());
    read_net(&db, move |lib| lib.art_choices(&opts, &row_id, kind)).await
}

/// Make one of those the row's picture.
#[tauri::command]
pub async fn choose_art(
    db: tauri::State<'_, Db>,
    row_id: String,
    kind: String,
    url: String,
    source: String,
) -> Res<()> {
    let kind = ArtKind::parse(&kind).ok_or_else(|| format!("no art kind {kind:?}"))?;
    let opts = EnrichOptions::new(default_cache_dir());
    edit(&db, move |lib| {
        lib.choose_art(&opts, &row_id, kind, &url, &source)
    })
    .await
}
