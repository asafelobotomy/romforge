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
    let detached = db.detached.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<PathBuf, romforge_core::Error> {
        // Background fetches on their own connection finish first.
        let _alone = detached.write().unwrap_or_else(|e| e.into_inner());
        let mut w = write.lock().unwrap_or_else(|e| e.into_inner());
        let mut r = read.lock().unwrap_or_else(|e| e.into_inner());
        let current = w.home().cloned();
        let old = db_path();
        // Both connections close before the database moves.
        *w = Library::open_in_memory()?;
        *r = Library::open_in_memory()?;
        let done = romforge_core::home::create_or_adopt(
            Path::new(&parent),
            current.as_ref(),
            &old,
            &default_cache_dir().join(romforge_core::layout::LOOSE_ART_DIR),
        );
        // Where the library is now: in the folder, unless that failed (then
        // where it was, or in the folder if it moved before the failure).
        let at = match &done {
            Ok(d) => Some(d.home().db()),
            Err(_) if old.is_file() => Some(old.clone()),
            Err(_) => Home::in_parent(Path::new(&parent))
                .map(|h| h.db())
                .filter(|p| p.is_file()),
        };
        let Some(at) = at else {
            return Err(romforge_core::Error::Settings(format!(
                "{}; the library couldn't be found again: restart romforge",
                done.err().map_or_else(String::new, |e| e.to_string())
            )));
        };
        reopen(&mut w, &mut r, &at, &old)?;
        roots.set(&w);
        let done = done?;
        done.home().remember()?;
        Ok(done.home().root().to_owned())
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
    let detached = db.detached.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<PathBuf, romforge_core::Error> {
        // Background fetches on their own connection finish first.
        let _alone = detached.write().unwrap_or_else(|e| e.into_inner());
        let mut w = write.lock().unwrap_or_else(|e| e.into_inner());
        let mut r = read.lock().unwrap_or_else(|e| e.into_inner());
        let Some(old) = w.home().cloned() else {
            return Err(romforge_core::Error::Settings("no RomForge folder yet".into()));
        };
        *w = Library::open_in_memory()?;
        *r = Library::open_in_memory()?;
        let moved = romforge_core::home::relocate(&old, Path::new(&parent));
        let at = moved.as_ref().map_or_else(|_| old.db(), |h| h.db());
        reopen(&mut w, &mut r, &at, &old.db())?;
        roots.set(&w);
        let new = moved?;
        new.remember()?;
        Ok(new.root().to_owned())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// Open both connections on `at`, else back on `fallback` (where the
/// library was), so a failure never leaves the app on an empty library in
/// memory; if neither opens, say to restart.
fn reopen(w: &mut Library, r: &mut Library, at: &Path, fallback: &Path) -> Result<(), romforge_core::Error> {
    let open = |p: &Path| -> Result<(Library, Library), romforge_core::Error> {
        Ok((Library::open(p)?, Library::open(p)?))
    };
    let (nw, nr) = match open(at) {
        Ok(pair) => pair,
        // Never a new, empty database where the library was.
        Err(first) if !fallback.is_file() => {
            return Err(romforge_core::Error::Settings(format!(
                "the library couldn't be opened again ({}: {first}); restart romforge",
                at.display()
            )));
        }
        Err(first) => open(fallback).map_err(|second| {
            romforge_core::Error::Settings(format!(
                "the library couldn't be opened again ({}: {first}; {}: {second}); restart romforge",
                at.display(),
                fallback.display()
            ))
        })?,
    };
    *w = nw;
    *r = nr;
    Ok(())
}
