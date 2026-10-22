//! A game's manual (milestone 26).

use std::path::PathBuf;

use romforge_core::manual::ManualInfo;

use crate::db::{Db, Res, blocking, edit, read, read_net, write_net};

#[tauri::command]
pub async fn manual_info(db: tauri::State<'_, Db>, row_id: String) -> Res<ManualInfo> {
    read(&db, move |lib| lib.manual_info(&row_id)).await
}

/// Ask Hasheous (once per game) for its Vimm's Lair manual id.
#[tauri::command]
pub async fn manual_lookup(db: tauri::State<'_, Db>, row_id: String) -> Res<Option<String>> {
    read_net(&db, move |lib| lib.manual_lookup(&row_id)).await
}

#[tauri::command]
pub async fn add_manual(db: tauri::State<'_, Db>, row_id: String, path: PathBuf) -> Res<PathBuf> {
    edit(&db, move |lib| lib.add_manual(&row_id, &path)).await
}

/// The manuals archive.org has for the game, best first.
#[tauri::command]
pub async fn manual_offers(
    db: tauri::State<'_, Db>,
    row_id: String,
) -> Res<Vec<romforge_core::manual::ManualOffer>> {
    read_net(&db, move |lib| lib.manual_offers(&row_id)).await
}

/// Download the manual the user picked: on its own connection
/// ([`write_net`]), so `write` stays free while it downloads.
#[tauri::command]
pub async fn download_manual_offer(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    row_id: String,
    offer: romforge_core::manual::ManualOffer,
    job: Option<u64>,
) -> Res<PathBuf> {
    let job = crate::jobs::Job::start(&app, job);
    write_net(&db, move |lib| {
        job.watch("manual", || lib.download_manual_offer(&row_id, &offer))
    })
    .await
}

/// Download it from Vimm's Lair: only ever on the user's click (on its
/// own connection, as [`download_manual_offer`]).
#[tauri::command]
pub async fn download_manual(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    row_id: String,
    dpi: u32,
    job: Option<u64>,
) -> Res<PathBuf> {
    let job = crate::jobs::Job::start(&app, job);
    write_net(&db, move |lib| {
        job.watch("manual", || lib.download_manual(&row_id, dpi))
    })
    .await
}

#[tauri::command]
pub async fn remove_manual(db: tauri::State<'_, Db>, row_id: String) -> Res<()> {
    edit(&db, move |lib| lib.remove_manual(&row_id)).await
}

#[tauri::command]
pub async fn set_manual_page(db: tauri::State<'_, Db>, row_id: String, page: u32) -> Res<()> {
    edit(&db, move |lib| lib.set_manual_page(&row_id, page)).await
}

#[tauri::command]
pub async fn manual_marks(db: tauri::State<'_, Db>, row_id: String) -> Res<Vec<romforge_core::manual::Mark>> {
    read(&db, move |lib| lib.manual_marks(&row_id)).await
}

#[tauri::command]
pub async fn add_manual_mark(
    db: tauri::State<'_, Db>,
    row_id: String,
    mark: romforge_core::manual::NewMark,
) -> Res<i64> {
    edit(&db, move |lib| lib.add_manual_mark(&row_id, &mark)).await
}

#[tauri::command]
pub async fn edit_manual_mark(db: tauri::State<'_, Db>, id: i64, note: String, color: String) -> Res<()> {
    edit(&db, move |lib| lib.edit_manual_mark(id, &note, &color)).await
}

#[tauri::command]
pub async fn remove_manual_mark(db: tauri::State<'_, Db>, id: i64) -> Res<()> {
    edit(&db, move |lib| lib.remove_manual_mark(id)).await
}

/// The words recognised on a scanned page, if it has been.
#[tauri::command]
pub async fn manual_ocr(
    db: tauri::State<'_, Db>,
    row_id: String,
    page: u32,
    lang: String,
) -> Res<Option<Vec<romforge_core::manual::OcrWord>>> {
    read(&db, move |lib| lib.manual_ocr(&row_id, page, &lang)).await
}

#[tauri::command]
pub async fn save_manual_ocr(
    db: tauri::State<'_, Db>,
    row_id: String,
    page: u32,
    lang: String,
    words: Vec<romforge_core::manual::OcrWord>,
) -> Res<()> {
    edit(&db, move |lib| lib.save_manual_ocr(&row_id, page, &lang, &words)).await
}

#[tauri::command]
pub async fn manual_ocr_texts(
    db: tauri::State<'_, Db>,
    row_id: String,
    lang: String,
) -> Res<Vec<(u32, String)>> {
    read(&db, move |lib| lib.manual_ocr_texts(&row_id, &lang)).await
}

/// The OCR languages, which are downloaded, and the folder they're in.
#[tauri::command]
pub async fn ocr_languages() -> Res<(Vec<romforge_core::manual::OcrLang>, PathBuf)> {
    blocking(|| {
        let cache = romforge_core::enrich::default_cache_dir();
        Ok((
            romforge_core::manual::ocr_langs(&cache),
            romforge_core::manual::ocr_dir(&cache),
        ))
    })
    .await
}

/// Download an OCR language (on the user's asking).
#[tauri::command]
pub async fn ocr_download(code: String) -> Res<PathBuf> {
    blocking(move || {
        romforge_core::manual::download_ocr_lang(&romforge_core::enrich::default_cache_dir(), &code)
    })
    .await
}
