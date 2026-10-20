//! Enrich, the art picker and the platform icons.

use romforge_core::enrich::{ArtKind, EnrichOptions, EnrichReport, default_cache_dir};

use crate::db::{Db, Res, read_net, with};

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
/// `system` (or of every platform).
#[tauri::command]
pub async fn enrich(
    db: tauri::State<'_, Db>,
    system: Option<String>,
    only: Option<Vec<String>>,
) -> Res<EnrichReport> {
    let opts = EnrichOptions {
        system,
        art: ArtKind::ALL.to_vec(),
        ..EnrichOptions::new(default_cache_dir())
    };
    with(&db, move |lib| match &only {
        Some(ids) => lib.enrich_rows(&opts, ids, |_| {}),
        None => lib.enrich(&opts, |_| {}),
    })
    .await
}

/// The pictures of `kind` the sources have for a row, each preview as a
/// `data:` URL (Details' art picker).
#[tauri::command]
pub async fn art_choices(
    db: tauri::State<'_, Db>,
    row_id: String,
    kind: String,
) -> Res<romforge_core::enrich::ArtChoices> {
    let kind = ArtKind::parse(&kind).ok_or_else(|| format!("no art kind {kind:?}"))?;
    let opts = EnrichOptions::new(default_cache_dir());
    // Previews stay file paths, shown through `art://`.
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
    with(&db, move |lib| {
        lib.choose_art(&opts, &row_id, kind, &url, &source)
    })
    .await
}
