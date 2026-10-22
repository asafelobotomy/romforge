//! The RomForge folder: where it is, making it and moving it.

use std::path::{Path, PathBuf};

use romforge_core::Library;
use romforge_core::enrich::default_cache_dir;
use romforge_core::home::Home;

use crate::art::ArtRoots;
use crate::db::{Db, Res, blocking, db_path, read};

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
            existing: lib.home().is_none() && !lib.is_empty()?,
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
    detached(&db, &roots, move |current, _| {
        let old = db_path();
        let parent = Path::new(&parent);
        let art = default_cache_dir().join(romforge_core::layout::LOOSE_ART_DIR);
        let done = romforge_core::home::create_or_adopt(parent, current.as_ref(), &old, &art);
        let Some(at) = romforge_core::home::db_after_create(parent, &old, &done) else {
            return Err(romforge_core::Error::Settings(format!(
                "{}; the library couldn't be found again: restart romforge",
                done.err().map_or_else(String::new, |e| e.to_string())
            )));
        };
        let finish = move || {
            let done = done?;
            done.home().remember()?;
            Ok(done.home().root().to_owned())
        };
        Ok((at, old, Box::new(finish) as Finish))
    })
    .await
}

/// Move the RomForge folder into `parent`: both connections close, the
/// folder moves, and the library reopens there (fixing its stored paths).
#[tauri::command]
pub async fn home_move(
    db: tauri::State<'_, Db>,
    roots: tauri::State<'_, ArtRoots>,
    parent: String,
) -> Res<PathBuf> {
    detached(&db, &roots, move |current, was| {
        let Some(old) = current else {
            let no = || Err(romforge_core::Error::Settings("no RomForge folder yet".into()));
            return Ok((was.clone(), was, Box::new(no) as Finish));
        };
        let moved = romforge_core::home::relocate(&old, Path::new(&parent));
        let at = romforge_core::home::db_after_move(&old, &moved);
        let finish = move || {
            let new = moved?;
            new.remember()?;
            Ok(new.root().to_owned())
        };
        Ok((at, old.db(), Box::new(finish) as Finish))
    })
    .await
}

/// What a move says once both connections are open again.
type Finish = Box<dyn FnOnce() -> Result<PathBuf, romforge_core::Error> + Send>;

/// Run `change` (given the RomForge folder in use and where the database
/// is) with no connection open on the database: the background ones done
/// first, both of the app's in memory meanwhile. It moves the database
/// and says where it is now, where it was, and what to answer; both
/// connections then open there ([`reopen`]) and the art roots follow.
async fn detached(
    db: &tauri::State<'_, Db>,
    roots: &tauri::State<'_, ArtRoots>,
    change: impl FnOnce(Option<Home>, PathBuf) -> Result<(PathBuf, PathBuf, Finish), romforge_core::Error>
    + Send
    + 'static,
) -> Res<PathBuf> {
    let (write, read, roots) = (db.write.clone(), db.read.clone(), roots.inner().clone());
    let detached = db.detached.clone();
    blocking(move || {
        // Background fetches on their own connection finish first.
        let _alone = detached.write().unwrap_or_else(|e| e.into_inner());
        let mut w = write.lock().unwrap_or_else(|e| e.into_inner());
        let mut r = read.lock().unwrap_or_else(|e| e.into_inner());
        // Both connections close before the database moves.
        let current = w.home().cloned();
        let was = w.path().map_or_else(db_path, Path::to_owned);
        *w = Library::open_in_memory()?;
        *r = Library::open_in_memory()?;
        let (at, fallback, finish) = change(current, was)?;
        reopen(&mut w, &mut r, &at, &fallback)?;
        roots.set(&w);
        finish()
    })
    .await
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
