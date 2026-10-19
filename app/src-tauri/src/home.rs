//! The RomForge folder: where it is, making it and moving it.

use std::path::{Path, PathBuf};

use romforge_core::Library;
use romforge_core::enrich::default_cache_dir;
use romforge_core::home::Home;

use crate::art::ArtRoots;
use crate::db::{Db, Res, db_path, read};

#[derive(serde::Serialize)]
pub struct HomeInfo {
    /// The RomForge folder, if there is one yet.
    root: Option<PathBuf>,
    /// A library from before the RomForge folder, which moves into it.
    existing: bool,
    /// Where to suggest creating it: the home folder.
    suggested: Option<PathBuf>,
}

#[tauri::command]
pub async fn home_info(db: tauri::State<'_, Db>) -> Res<HomeInfo> {
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
pub async fn home_create(
    db: tauri::State<'_, Db>,
    roots: tauri::State<'_, ArtRoots>,
    parent: String,
) -> Res<PathBuf> {
    let (write, read, roots) = (db.write.clone(), db.read.clone(), roots.inner().clone());
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
            let moved = romforge_core::home::adopt(
                &home,
                &old,
                &default_cache_dir().join(romforge_core::layout::LOOSE_ART_DIR),
            );
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
        roots.set(&w);
        Ok(home.root().to_owned())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// Move the RomForge folder into `parent`: both connections close, the
/// folder moves, and the library reopens there (fixing its stored paths).
#[tauri::command]
pub async fn home_move(
    db: tauri::State<'_, Db>,
    roots: tauri::State<'_, ArtRoots>,
    parent: String,
) -> Res<PathBuf> {
    let (write, read, roots) = (db.write.clone(), db.read.clone(), roots.inner().clone());
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
        roots.set(&w);
        Ok(moved?.root().to_owned())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}
