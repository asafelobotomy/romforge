//! Search: identifying by hand and settling Wikidata matches.

use std::path::PathBuf;

use romforge_core::enrich::{ArtKind, EnrichOptions, default_cache_dir};
use romforge_core::search::{Chosen, Found, Preview, QueueItem, Scraper};

use crate::db::{Db, Res, blocking, read, read_net, with};
use crate::images::image_url;

#[tauri::command]
pub async fn search_queue(db: tauri::State<'_, Db>, only: Option<Vec<String>>) -> Res<Vec<QueueItem>> {
    read(&db, move |lib| lib.search_queue(only.as_deref())).await
}

/// Name rows by their strongest suggestion (Search's "Name as suggested").
#[tauri::command]
pub async fn search_accept(
    db: tauri::State<'_, Db>,
    only: Vec<String>,
) -> Res<Vec<romforge_core::search::Accepted>> {
    with(&db, move |lib| lib.search_accept(&only)).await
}

#[tauri::command]
pub async fn forget_identity(db: tauri::State<'_, Db>, row_id: String) -> Res<()> {
    with(&db, move |lib| lib.forget_identity(&row_id)).await
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

/// The preview with its picture as a data: URL (in `image`).
#[tauri::command]
pub async fn search_preview(db: tauri::State<'_, Db>, row_id: String, found: Found) -> Res<Preview> {
    let mut p = read_net(&db, move |lib| {
        lib.search_preview(
            &row_id,
            &found,
            &default_cache_dir(),
            romforge_core::enrich::THUMBNAILS_BASE,
        )
    })
    .await?;
    // Reading and encoding the picture is blocking work too.
    blocking(move || {
        p.image = p.image.and_then(|path| image_url(&path)).map(PathBuf::from);
        Ok(p)
    })
    .await
}

/// Take a result, then fetch `art` kinds (and Wikidata) for that game;
/// `overwrite` fetches again what is already there.
#[tauri::command]
pub async fn search_choose(
    db: tauri::State<'_, Db>,
    row_id: String,
    found: Found,
    system: Option<String>,
    art: Vec<String>,
    wikidata: bool,
    overwrite: bool,
) -> Res<Chosen> {
    let opts = EnrichOptions {
        art: art.iter().filter_map(|k| ArtKind::parse(k)).collect(),
        wikidata: wikidata.then(|| romforge_core::wikidata::ENDPOINT.to_owned()),
        refresh: overwrite,
        ..EnrichOptions::new(default_cache_dir())
    };
    with(&db, move |lib| {
        lib.search_choose(&row_id, &found, system.as_deref(), &opts)
    })
    .await
}
