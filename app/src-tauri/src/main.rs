//! Tauri shell over romforge-core. Every command is a call into the core.
//! Each runs on a blocking thread (never the main thread or an async
//! worker), and commands that only read use a second connection: SQLite's
//! WAL lets them go on while a scan, Enrich or a download writes.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use romforge_core::cleanup::{Batch as CleanupBatch, CleanupPlan, CleanupReport, CleanupUndone};
use romforge_core::enrich::{ArtKind, EnrichOptions, EnrichReport, default_cache_dir};
use romforge_core::extdb::{ExtDb, ExtDbInfo};
use romforge_core::home::Home;
use romforge_core::launch::Env;
use romforge_core::organize::{Applied, OrganizeOptions, Plan, Undone};
use romforge_core::search::{Chosen, Found, Preview, QueueItem, Scraper};
use romforge_core::settings::{Settings, default_staging_dir};
use romforge_core::view::{GameDetail, LibraryView};
use romforge_core::{DatGame, Library, ScanReport, Scope, Summary};

/// The library twice: `write` for commands that change it, `read` for
/// those that only look (a second connection to the same database).
struct Db {
    write: Arc<Mutex<Library>>,
    read: Arc<Mutex<Library>>,
}

type Res<T> = Result<T, String>;

/// Run `f` on a blocking thread with `lib` locked. A panic in an earlier
/// call doesn't lock everyone out (the poisoned lock is taken anyway).
async fn on<T: Send + 'static>(
    lib: &Arc<Mutex<Library>>,
    f: impl FnOnce(&mut Library) -> Result<T, romforge_core::Error> + Send + 'static,
) -> Res<T> {
    let lib = lib.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut lib = lib.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut lib).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// For commands that change the library.
async fn with<T: Send + 'static>(
    db: &tauri::State<'_, Db>,
    f: impl FnOnce(&mut Library) -> Result<T, romforge_core::Error> + Send + 'static,
) -> Res<T> {
    on(&db.write, f).await
}

/// For commands that only read it.
async fn read<T: Send + 'static>(
    db: &tauri::State<'_, Db>,
    f: impl FnOnce(&mut Library) -> Result<T, romforge_core::Error> + Send + 'static,
) -> Res<T> {
    on(&db.read, f).await
}

#[tauri::command]
async fn summary(db: tauri::State<'_, Db>) -> Res<Summary> {
    read(&db, move |lib| lib.summary()).await
}

#[tauri::command]
async fn dat_games(db: tauri::State<'_, Db>, dat_id: i64) -> Res<Vec<DatGame>> {
    read(&db, move |lib| lib.dat_games(dat_id)).await
}

#[tauri::command]
async fn scan(db: tauri::State<'_, Db>, dir: String) -> Res<ScanReport> {
    with(&db, move |lib| lib.scan(&dir, Scope::All)).await
}

#[derive(serde::Serialize)]
struct Downloaded {
    imported: usize,
    unchanged: usize,
    errors: Vec<String>,
}

async fn extdb_dir(db: &tauri::State<'_, Db>) -> Res<PathBuf> {
    read(db, |lib| Ok(lib.extdb_dir()))
        .await?
        .ok_or_else(|| "no folder for databases".into())
}

fn extdb_named(name: &str) -> Res<ExtDb> {
    ExtDb::parse(name).ok_or_else(|| format!("unknown database {name:?}"))
}

#[tauri::command]
async fn extdb_list(db: tauri::State<'_, Db>) -> Res<Vec<ExtDbInfo>> {
    Ok(romforge_core::extdb::list(&extdb_dir(&db).await?))
}

/// Downloads without holding the library, so the window stays usable.
#[tauri::command]
async fn extdb_download(db: tauri::State<'_, Db>, name: String) -> Res<ExtDbInfo> {
    let dir = extdb_dir(&db).await?;
    romforge_core::extdb::download(extdb_named(&name)?, &dir, &Default::default(), |_| {})
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn extdb_remove(db: tauri::State<'_, Db>, name: String) -> Res<()> {
    let dir = extdb_dir(&db).await?;
    romforge_core::extdb::remove(extdb_named(&name)?, &dir).map_err(|e| e.to_string())
}

#[tauri::command]
async fn download_dats(db: tauri::State<'_, Db>, extras: bool) -> Res<Downloaded> {
    with(&db, move |lib| {
        let mut out = Downloaded {
            imported: 0,
            unchanged: 0,
            errors: Vec::new(),
        };
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
        })?;
        Ok(out)
    })
    .await
}

fn organize_options(from: &str, dest: &str, one_g1r: bool) -> OrganizeOptions {
    OrganizeOptions {
        one_g1r,
        ..OrganizeOptions::new(from, dest)
    }
}

#[tauri::command]
async fn organize_plan(db: tauri::State<'_, Db>, from: String, dest: String, one_g1r: bool) -> Res<Plan> {
    read(&db, move |lib| {
        lib.organize_plan(&organize_options(&from, &dest, one_g1r))
    })
    .await
}

#[tauri::command]
async fn organize_apply(db: tauri::State<'_, Db>, from: String, dest: String, one_g1r: bool) -> Res<Applied> {
    with(&db, move |lib| {
        let plan = lib.organize_plan(&organize_options(&from, &dest, one_g1r))?;
        lib.organize_apply(&plan, &PathBuf::from(&dest))
    })
    .await
}

#[tauri::command]
async fn organize_undo(db: tauri::State<'_, Db>) -> Res<Undone> {
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
async fn cleanup_plan(
    db: tauri::State<'_, Db>,
    only: Option<Vec<String>>,
    from: Option<String>,
) -> Res<CleanupPlan> {
    read(&db, move |lib| plan_for(lib, only.as_deref(), from.as_deref())).await
}

/// Plans again here rather than trusting a plan sent back from the window.
#[tauri::command]
async fn cleanup_apply(
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

#[derive(serde::Serialize)]
struct HomeInfo {
    /// The RomForge folder, if there is one yet.
    root: Option<PathBuf>,
    /// A library from before the RomForge folder, which moves into it.
    existing: bool,
    /// Where to suggest creating it: the home folder.
    suggested: Option<PathBuf>,
}

#[tauri::command]
async fn home_info(db: tauri::State<'_, Db>) -> Res<HomeInfo> {
    read(&db, |lib| {
        Ok(HomeInfo {
            root: lib.home().map(|h| h.root().to_owned()),
            existing: lib.home().is_none() && lib.library_view()?.rows.len() + lib.summary()?.dats.len() > 0,
            suggested: std::env::var_os("HOME").map(PathBuf::from),
        })
    })
    .await
}

/// Create the RomForge folder in `parent`, move the current library into
/// it (database, art, downloaded databases) and use it from now on.
#[tauri::command]
async fn home_create(db: tauri::State<'_, Db>, parent: String) -> Res<PathBuf> {
    let (write, read) = (db.write.clone(), db.read.clone());
    tauri::async_runtime::spawn_blocking(move || -> Result<PathBuf, romforge_core::Error> {
        let mut w = write.lock().unwrap_or_else(|e| e.into_inner());
        let mut r = read.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = w.home() {
            return Ok(h.root().to_owned());
        }
        let old = db_path();
        let home = Home::create(Path::new(&parent))?;
        if !home.db().exists() {
            // Both connections close before the database moves.
            *w = Library::open_in_memory()?;
            *r = Library::open_in_memory()?;
            let moved = romforge_core::home::adopt(&home, &old, &default_cache_dir().join("art"));
            let reopen = if home.db().exists() {
                home.db()
            } else {
                old.clone()
            };
            *w = Library::open(&reopen)?;
            *r = Library::open(&reopen)?;
            moved?;
        } else {
            *w = Library::open(home.db())?;
            *r = Library::open(home.db())?;
        }
        home.remember()?;
        Ok(home.root().to_owned())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// Move the RomForge folder into `parent`: both connections close, the
/// folder moves, and the library reopens there (fixing its stored paths).
#[tauri::command]
async fn home_move(db: tauri::State<'_, Db>, parent: String) -> Res<PathBuf> {
    let (write, read) = (db.write.clone(), db.read.clone());
    tauri::async_runtime::spawn_blocking(move || -> Result<PathBuf, romforge_core::Error> {
        let mut w = write.lock().unwrap_or_else(|e| e.into_inner());
        let mut r = read.lock().unwrap_or_else(|e| e.into_inner());
        let Some(old) = w.home().cloned() else {
            return Err(romforge_core::Error::Settings("no RomForge folder yet".into()));
        };
        *w = Library::open_in_memory()?;
        *r = Library::open_in_memory()?;
        let moved =
            romforge_core::home::relocate(&old, Path::new(&parent)).and_then(|h| h.remember().map(|()| h));
        let at = moved.as_ref().map_or_else(|_| old.db(), |h| h.db());
        *w = Library::open(&at)?;
        *r = Library::open(&at)?;
        Ok(moved?.root().to_owned())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// `$ROMFORGE_DB`, else the RomForge folder's database, else the old
/// default.
fn db_path() -> PathBuf {
    std::env::var_os("ROMFORGE_DB")
        .map(PathBuf::from)
        .or_else(|| Home::locate().map(|h| h.db()))
        .unwrap_or_else(romforge_core::default_db_path)
}

#[tauri::command]
async fn cleanup_undo(db: tauri::State<'_, Db>) -> Res<CleanupUndone> {
    with(&db, move |lib| lib.cleanup_undo(None)).await
}

#[tauri::command]
async fn cleanup_batches(db: tauri::State<'_, Db>) -> Res<Vec<CleanupBatch>> {
    read(&db, move |lib| lib.cleanup_batches()).await
}

#[tauri::command]
async fn enrich(db: tauri::State<'_, Db>, system: Option<String>) -> Res<EnrichReport> {
    let opts = EnrichOptions {
        system,
        art: ArtKind::ALL.to_vec(),
        ..EnrichOptions::new(default_cache_dir())
    };
    with(&db, move |lib| lib.enrich(&opts, |_| {})).await
}

#[tauri::command]
async fn get_settings(db: tauri::State<'_, Db>) -> Res<Settings> {
    read(&db, move |lib| lib.settings_masked()).await
}

#[tauri::command]
async fn save_settings(db: tauri::State<'_, Db>, settings: Settings) -> Res<Settings> {
    with(&db, move |lib| lib.save_settings_from_form(&settings)).await
}

#[derive(serde::Serialize)]
struct SettingsInfo {
    default_staging: String,
}

/// Read ScummVM's detection list from the installed ScummVM, or from
/// `file` (one saved with `scummvm --dump-all-detection-entries`).
#[tauri::command]
async fn scummvm_list(db: tauri::State<'_, Db>, file: Option<String>) -> Res<romforge_core::ScummvmList> {
    with(&db, move |lib| match file {
        Some(f) => lib.scummvm_import(&std::fs::read(f)?),
        None => lib.scummvm_read(&romforge_core::launch::Env::detect()),
    })
    .await
}

#[tauri::command]
async fn settings_info() -> Res<SettingsInfo> {
    Ok(SettingsInfo {
        default_staging: default_staging_dir().display().to_string(),
    })
}

#[tauri::command]
async fn search_queue(db: tauri::State<'_, Db>) -> Res<Vec<QueueItem>> {
    read(&db, move |lib| lib.search_queue(None)).await
}

#[tauri::command]
async fn forget_identity(db: tauri::State<'_, Db>, row_id: String) -> Res<()> {
    with(&db, move |lib| lib.forget_identity(&row_id)).await
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
    read(&db, move |lib| lib.library_view()).await
}

#[tauri::command]
async fn game_detail(db: tauri::State<'_, Db>, id: String) -> Res<GameDetail> {
    let mut d = read(&db, move |lib| lib.game_detail(&id)).await?;
    // Paths become data: URLs the page can show without file access.
    for path in d.art.values_mut() {
        *path = image_url(Path::new(&*path)).unwrap_or_default();
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
    read(&db, move |lib| {
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
async fn search_preview(db: tauri::State<'_, Db>, row_id: String, found: Found) -> Res<Preview> {
    let mut p = read(&db, move |lib| {
        lib.search_preview(
            &row_id,
            &found,
            &default_cache_dir(),
            romforge_core::enrich::THUMBNAILS_BASE,
        )
    })
    .await?;
    p.image = p.image.and_then(|path| image_url(&path)).map(PathBuf::from);
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
    with(&db, move |lib| {
        lib.search_choose(&row_id, &found, system.as_deref(), &opts)
    })
    .await
}

#[derive(serde::Serialize)]
struct Refreshed {
    folders: usize,
    entries: usize,
    problems: Vec<String>,
}

#[tauri::command]
async fn refresh(db: tauri::State<'_, Db>) -> Res<Refreshed> {
    let results = with(&db, move |lib| lib.refresh(Default::default(), |_, _| {})).await?;
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

#[tauri::command]
/// Start one library row (exactly its files).
async fn launch(db: tauri::State<'_, Db>, row_id: String) -> Res<String> {
    let spec = read(&db, move |lib| lib.launch_row(&Env::detect(), &row_id, None)).await?;
    romforge_core::launch::spawn(&spec).map_err(|e| e.to_string())?;
    Ok(format!("{} started with {}", spec.game, spec.emulator))
}

/// Pictures over this are left out of Details: every one is sent to the
/// page as base64 (a third larger), and a fanart can be tens of MB.
const MAX_IMAGE: u64 = 8 << 20;

/// The image at `path` as a `data:` URL, if it is there and not too big.
fn image_url(path: &Path) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > MAX_IMAGE {
        return None;
    }
    Some(data_url(&std::fs::read(path).ok()?))
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
    let path = db_path();
    let open = || Library::open(&path).unwrap_or_else(|e| panic!("opening {}: {e}", path.display()));
    let db = Db {
        write: Arc::new(Mutex::new(open())),
        read: Arc::new(Mutex::new(open())),
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(db)
        .invoke_handler(tauri::generate_handler![
            summary,
            dat_games,
            scan,
            download_dats,
            extdb_list,
            extdb_download,
            extdb_remove,
            organize_plan,
            organize_apply,
            organize_undo,
            cleanup_plan,
            cleanup_apply,
            cleanup_undo,
            cleanup_batches,
            home_info,
            home_create,
            home_move,
            enrich,
            launch,
            library_view,
            game_detail,
            refresh,
            get_settings,
            save_settings,
            settings_info,
            scummvm_list,
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
