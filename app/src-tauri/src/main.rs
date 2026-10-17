//! Tauri shell over romforge-core. Every command is a call into the core.
//! All are async: a sync command runs on the main thread, and would freeze
//! the window waiting for the library while a long scan or download holds it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use romforge_core::cleanup::{Batch as CleanupBatch, CleanupPlan, CleanupReport, CleanupUndone};
use romforge_core::enrich::{ArtKind, EnrichOptions, EnrichReport, default_cache_dir};
use romforge_core::launch::Env;
use romforge_core::organize::{Applied, OrganizeOptions, Plan, Undone};
use romforge_core::search::{Chosen, Found, Preview, QueueItem, Scraper};
use romforge_core::settings::{Settings, default_staging_dir};
use romforge_core::view::{GameDetail, LibraryView};
use romforge_core::{
    DatGame, Library, LibraryFile, LibraryFilter, ScanReport, Scope, Status, Summary, SystemInfo,
};

struct Db(Mutex<Library>);

type Res<T> = Result<T, String>;

fn with<T>(db: &tauri::State<Db>, f: impl FnOnce(&mut Library) -> Result<T, romforge_core::Error>) -> Res<T> {
    let mut lib = db.0.lock().map_err(|e| e.to_string())?;
    f(&mut lib).map_err(|e| e.to_string())
}

#[tauri::command]
async fn library(
    db: tauri::State<'_, Db>,
    system_id: Option<i64>,
    status: Option<String>,
) -> Res<Vec<LibraryFile>> {
    let status = match status {
        Some(s) => Some(Status::parse(&s).ok_or_else(|| format!("unknown status {s:?}"))?),
        None => None,
    };
    with(&db, |lib| lib.files(&LibraryFilter { system_id, status }))
}

#[tauri::command]
async fn systems(db: tauri::State<'_, Db>) -> Res<Vec<SystemInfo>> {
    with(&db, |lib| lib.systems_in_use())
}

#[tauri::command]
async fn summary(db: tauri::State<'_, Db>) -> Res<Summary> {
    with(&db, |lib| lib.summary())
}

#[tauri::command]
async fn dat_games(db: tauri::State<'_, Db>, dat_id: i64) -> Res<Vec<DatGame>> {
    with(&db, |lib| lib.dat_games(dat_id))
}

#[tauri::command]
async fn scan(db: tauri::State<'_, Db>, dir: String) -> Res<ScanReport> {
    with(&db, |lib| lib.scan(&dir, Scope::All))
}

#[derive(serde::Serialize)]
struct Downloaded {
    imported: usize,
    unchanged: usize,
    errors: Vec<String>,
}

#[tauri::command]
async fn download_dats(db: tauri::State<'_, Db>, extras: bool) -> Res<Downloaded> {
    let mut out = Downloaded {
        imported: 0,
        unchanged: 0,
        errors: Vec::new(),
    };
    with(&db, |lib| {
        lib.download_all(extras, |key, source, r| match r {
            Ok(dats) => {
                for d in dats {
                    if d.unchanged {
                        out.unchanged += 1;
                    } else {
                        out.imported += 1;
                    }
                }
            }
            Err(e) => out.errors.push(format!("{key}: {source}: {e}")),
        })
    })?;
    Ok(out)
}

fn organize_options(from: &str, dest: &str, one_g1r: bool) -> OrganizeOptions {
    OrganizeOptions {
        one_g1r,
        ..OrganizeOptions::new(from, dest)
    }
}

#[tauri::command]
async fn organize_plan(db: tauri::State<'_, Db>, from: String, dest: String, one_g1r: bool) -> Res<Plan> {
    with(&db, |lib| {
        lib.organize_plan(&organize_options(&from, &dest, one_g1r))
    })
}

#[tauri::command]
async fn organize_apply(db: tauri::State<'_, Db>, from: String, dest: String, one_g1r: bool) -> Res<Applied> {
    with(&db, |lib| {
        let plan = lib.organize_plan(&organize_options(&from, &dest, one_g1r))?;
        lib.organize_apply(&plan, &PathBuf::from(&dest))
    })
}

#[tauri::command]
async fn organize_undo(db: tauri::State<'_, Db>) -> Res<Undone> {
    with(&db, |lib| lib.organize_undo(None))
}

/// `only`: these library row ids, else every identified game in Source.
#[tauri::command]
async fn cleanup_plan(db: tauri::State<'_, Db>, only: Option<Vec<String>>) -> Res<CleanupPlan> {
    with(&db, |lib| lib.cleanup_plan(only.as_deref()))
}

/// Plans again here rather than trusting a plan sent back from the window.
#[tauri::command]
async fn cleanup_apply(db: tauri::State<'_, Db>, only: Option<Vec<String>>) -> Res<CleanupReport> {
    with(&db, |lib| {
        let plan = lib.cleanup_plan(only.as_deref())?;
        lib.cleanup_apply(&plan, |_, _| {})
    })
}

#[tauri::command]
async fn cleanup_undo(db: tauri::State<'_, Db>) -> Res<CleanupUndone> {
    with(&db, |lib| lib.cleanup_undo(None))
}

#[tauri::command]
async fn cleanup_batches(db: tauri::State<'_, Db>) -> Res<Vec<CleanupBatch>> {
    with(&db, |lib| lib.cleanup_batches())
}

#[tauri::command]
async fn enrich(db: tauri::State<'_, Db>, system: Option<String>) -> Res<EnrichReport> {
    let opts = EnrichOptions {
        system,
        art: ArtKind::ALL.to_vec(),
        ..EnrichOptions::new(default_cache_dir())
    };
    with(&db, |lib| lib.enrich(&opts, |_| {}))
}

#[tauri::command]
async fn get_settings(db: tauri::State<'_, Db>) -> Res<Settings> {
    with(&db, |lib| lib.settings())
}

#[tauri::command]
async fn save_settings(db: tauri::State<'_, Db>, settings: Settings) -> Res<Settings> {
    with(&db, |lib| lib.save_settings(&settings))
}

#[derive(serde::Serialize)]
struct SettingsInfo {
    default_staging: String,
}

#[tauri::command]
async fn settings_info() -> Res<SettingsInfo> {
    Ok(SettingsInfo {
        default_staging: default_staging_dir().display().to_string(),
    })
}

#[tauri::command]
async fn search_queue(db: tauri::State<'_, Db>) -> Res<Vec<QueueItem>> {
    with(&db, |lib| lib.search_queue(None))
}

#[tauri::command]
async fn forget_identity(db: tauri::State<'_, Db>, row_id: String) -> Res<()> {
    with(&db, |lib| lib.forget_identity(&row_id))
}

#[derive(serde::Serialize)]
struct SystemChoice {
    key: &'static str,
    name: &'static str,
}

/// Every platform, for picking one by hand.
#[tauri::command]
async fn all_systems() -> Res<Vec<SystemChoice>> {
    let mut v: Vec<SystemChoice> = romforge_core::systems::SYSTEMS
        .iter()
        .map(|s| SystemChoice {
            key: s.key,
            name: s.name,
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(b.name));
    Ok(v)
}

#[tauri::command]
async fn library_view(db: tauri::State<'_, Db>) -> Res<LibraryView> {
    with(&db, |lib| lib.library_view())
}

#[tauri::command]
async fn game_detail(db: tauri::State<'_, Db>, id: String) -> Res<GameDetail> {
    let mut d = with(&db, |lib| lib.game_detail(&id))?;
    // Paths become data: URLs the page can show without file access.
    for path in d.art.values_mut() {
        *path = match std::fs::read(&*path) {
            Ok(img) => data_url(&img),
            Err(_) => String::new(),
        };
    }
    d.art.retain(|_, v| !v.is_empty());
    Ok(d)
}

#[tauri::command]
async fn search_find(
    db: tauri::State<'_, Db>,
    row_id: String,
    scraper: Scraper,
    text: Option<String>,
    system: Option<String>,
) -> Res<Vec<Found>> {
    with(&db, |lib| {
        lib.search_find(
            &row_id,
            scraper,
            text.as_deref(),
            system.as_deref(),
            romforge_core::search::WIKIDATA_API,
        )
    })
}

/// The preview with its picture as a data: URL (in `image`).
#[tauri::command]
async fn search_preview(db: tauri::State<'_, Db>, row_id: String, found: Found) -> Res<Preview> {
    let mut p = with(&db, |lib| {
        lib.search_preview(
            &row_id,
            &found,
            &default_cache_dir(),
            romforge_core::enrich::THUMBNAILS_BASE,
        )
    })?;
    p.image = p
        .image
        .and_then(|path| std::fs::read(path).ok())
        .map(|img| PathBuf::from(data_url(&img)));
    Ok(p)
}

/// Take a result, then fetch `art` kinds (and Wikidata) for that game;
/// `overwrite` fetches again what is already there.
#[tauri::command]
async fn search_choose(
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
    with(&db, |lib| {
        lib.search_choose(&row_id, &found, system.as_deref(), &opts)
    })
}

#[derive(serde::Serialize)]
struct Refreshed {
    folders: usize,
    entries: usize,
    problems: Vec<String>,
}

#[tauri::command]
async fn refresh(db: tauri::State<'_, Db>) -> Res<Refreshed> {
    let results = with(&db, |lib| lib.refresh(Default::default(), |_, _| {}))?;
    let mut out = Refreshed {
        folders: 0,
        entries: 0,
        problems: Vec::new(),
    };
    for r in results {
        match r {
            Ok(report) => {
                out.folders += 1;
                out.entries += report.hashed;
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
    Ok(out)
}

#[derive(serde::Serialize)]
struct Info {
    title: String,
    system: String,
    fields: BTreeMap<String, String>,
    /// kind -> `data:image/...;base64,...`
    art: BTreeMap<String, String>,
}

#[tauri::command]
async fn game_info(db: tauri::State<'_, Db>, query: String) -> Res<Vec<Info>> {
    let found = with(&db, |lib| lib.game_info(&query))?;
    Ok(found
        .into_iter()
        .map(|g| Info {
            art: g
                .art
                .iter()
                .filter_map(|(k, p)| {
                    let png = std::fs::read(p).ok()?;
                    Some((k.as_str().to_owned(), data_url(&png)))
                })
                .collect(),
            title: g.title,
            system: g.system,
            fields: g.fields,
        })
        .collect())
}

#[tauri::command]
async fn launch(db: tauri::State<'_, Db>, query: String) -> Res<String> {
    let spec = with(&db, |lib| lib.launch_plan(&Env::detect(), &query, None))?;
    romforge_core::launch::spawn(&spec).map_err(|e| e.to_string())?;
    Ok(format!("{} started with {}", spec.game, spec.emulator))
}

/// An image as a `data:` URL with its real type (PNG, JPEG or WebP).
fn data_url(img: &[u8]) -> String {
    let mime = match romforge_core::enrich::image_ext(img) {
        Some("jpg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    format!("data:{mime};base64,{}", base64(img))
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn main() {
    let path = std::env::var_os("ROMFORGE_DB")
        .map(PathBuf::from)
        .unwrap_or_else(romforge_core::default_db_path);
    let lib = Library::open(&path).unwrap_or_else(|e| panic!("opening {}: {e}", path.display()));
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Db(Mutex::new(lib)))
        .invoke_handler(tauri::generate_handler![
            library,
            systems,
            summary,
            dat_games,
            scan,
            download_dats,
            organize_plan,
            organize_apply,
            organize_undo,
            cleanup_plan,
            cleanup_apply,
            cleanup_undo,
            cleanup_batches,
            enrich,
            game_info,
            launch,
            library_view,
            game_detail,
            refresh,
            get_settings,
            save_settings,
            settings_info,
            search_queue,
            search_find,
            search_preview,
            search_choose,
            forget_identity,
            all_systems
        ])
        .run(tauri::generate_context!())
        .expect("error while running romforge");
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foo"), "Zm9v");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
    }
}
