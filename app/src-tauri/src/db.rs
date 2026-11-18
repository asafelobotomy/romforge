//! The library's two connections, and running calls on them.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockWriteGuard, TryLockError, TryLockResult};
use std::time::{Duration, Instant};

use romforge_core::Library;

/// The library twice: `write` for commands that change it, `read` for
/// those that only look (a second connection to the same database).
/// `detached`: held shared by [`read_net`], [`write_net`] and [`edit`]
/// calls while their own connection is open, and exclusively by a move of
/// the database (taken before `write` and `read`, in that order everywhere).
pub struct Db {
    pub write: Arc<Mutex<Library>>,
    pub read: Arc<Mutex<Library>>,
    pub detached: Arc<RwLock<()>>,
}

pub type Res<T> = Result<T, String>;

/// Run `f` on a blocking thread with `lib` locked. A panic in an earlier
/// call doesn't lock everyone out (the poisoned lock is taken anyway).
pub async fn on<T: Send + 'static>(
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

/// Run `f` (file or network work that needs no connection) on a blocking
/// thread.
pub async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, romforge_core::Error> + Send + 'static,
) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// For commands that change the library.
pub async fn with<T: Send + 'static>(
    db: &tauri::State<'_, Db>,
    f: impl FnOnce(&mut Library) -> Result<T, romforge_core::Error> + Send + 'static,
) -> Res<T> {
    on(&db.write, f).await
}

/// For commands that only read it.
pub async fn read<T: Send + 'static>(
    db: &tauri::State<'_, Db>,
    f: impl FnOnce(&mut Library) -> Result<T, romforge_core::Error> + Send + 'static,
) -> Res<T> {
    on(&db.read, f).await
}

/// For reads that wait on the network or run long (searches, previews,
/// art choices, platform icons, core and BIOS-list downloads, installs):
/// their own short-lived connection, so `read` stays free for every other
/// command meanwhile. It opens where `read` is, holding `detached` so a
/// move of the RomForge folder waits for it.
pub async fn read_net<T: Send + 'static>(
    db: &tauri::State<'_, Db>,
    f: impl FnOnce(&mut Library) -> Result<T, romforge_core::Error> + Send + 'static,
) -> Res<T> {
    let (read, detached) = (db.read.clone(), db.detached.clone());
    blocking(move || apart(&read, &detached, |at| f(&mut Library::open_beside(at)?))).await
}

/// For writes that mostly wait on the network (Search's fetch after a
/// pick): their own connection, so `write` stays free while they download.
/// SQLite (WAL) takes its write lock only for each save, which happens
/// after its fetch.
pub async fn write_net<T: Send + 'static>(
    db: &tauri::State<'_, Db>,
    f: impl FnOnce(&mut Library) -> Result<T, romforge_core::Error> + Send + 'static,
) -> Res<T> {
    let (read, detached) = (db.read.clone(), db.detached.clone());
    blocking(move || apart(&read, &detached, |at| f(&mut Library::open_beside(at)?))).await
}

/// For quick edits (forget a match, pick art, save settings...): their
/// own connection ([`Library::edit`]), so they never wait on `write`,
/// which a long job (Enrich, cleanup) holds for its whole run. A scan,
/// refresh or DAT import also holds SQLite's write lock that long; then
/// the edit gives up after a moment with an error starting `busy:`, and
/// the window queues it behind that job (jobs.svelte.js `edit`).
pub async fn edit<T: Send + 'static>(
    db: &tauri::State<'_, Db>,
    f: impl FnOnce(&mut Library) -> Result<T, romforge_core::Error> + Send + 'static,
) -> Res<T> {
    let (read, detached) = (db.read.clone(), db.detached.clone());
    blocking(move || apart(&read, &detached, |at| Library::edit(at, f))).await
}

/// For file work in the RomForge folder that needs no connection
/// (downloaded databases, themes): `f` runs holding `detached`, so a move
/// of the folder waits for it, and it looks up its place inside (from
/// `read`, locked only for that), so it never writes where the folder was.
pub async fn in_folder<T: Send + 'static>(
    db: &tauri::State<'_, Db>,
    f: impl FnOnce(&Mutex<Library>) -> Result<T, romforge_core::Error> + Send + 'static,
) -> Res<T> {
    let (read, detached) = (db.read.clone(), db.detached.clone());
    blocking(move || {
        let _open = detached.read().unwrap_or_else(|e| e.into_inner());
        f(&read)
    })
    .await
}

/// How long a move of the RomForge folder waits for the job or download
/// holding the library before it says so.
pub const MOVE_WAIT: Duration = Duration::from_secs(2);

/// What a move of the database holds: `detached` (no download, edit or
/// network read on its own connection), `write` and `read`.
pub(crate) struct Alone<'a> {
    _detached: RwLockWriteGuard<'a, ()>,
    pub write: MutexGuard<'a, Library>,
    pub read: MutexGuard<'a, Library>,
}

/// Take all three for a move, waiting at most `wait`. A move queued
/// behind a long job (a scan, Enrich) would hold `detached` meanwhile, and
/// every quick edit, network read and play-time save would wait with it;
/// so each is only tried, all are given back between tries, and whatever
/// still holds one after `wait` is named instead.
pub(crate) fn alone<'a>(
    write: &'a Mutex<Library>,
    read: &'a Mutex<Library>,
    detached: &'a RwLock<()>,
    wait: Duration,
) -> Result<Alone<'a>, romforge_core::Error> {
    let until = Instant::now() + wait;
    loop {
        let held = (|| {
            let d = taken(detached.try_write())
                .ok_or("a download or search is still running: move the RomForge folder once it ends")?;
            let w = taken(write.try_lock()).ok_or(
                "a job is running (a scan, Enrich, cleanup…): move the RomForge folder once it ends, or stop it",
            )?;
            let r = taken(read.try_lock()).ok_or("the library is busy: try again in a moment")?;
            Ok::<_, &str>(Alone {
                _detached: d,
                write: w,
                read: r,
            })
        })();
        match held {
            Ok(alone) => return Ok(alone),
            Err(why) if Instant::now() >= until => return Err(romforge_core::Error::Settings(why.into())),
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// A lock if it was free (a poisoned one too: an earlier panic doesn't
/// lock everyone out).
fn taken<G>(r: TryLockResult<G>) -> Option<G> {
    match r {
        Ok(g) => Some(g),
        Err(TryLockError::Poisoned(e)) => Some(e.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

/// Run `f` with where the database is, holding `detached` so a move of
/// it waits for `f`. The place is `read`'s (never asked of `write`, which
/// a long job holds): both connections always open at the same place.
pub(crate) fn apart<T>(
    read: &Mutex<Library>,
    detached: &RwLock<()>,
    f: impl FnOnce(&Path) -> Result<T, romforge_core::Error>,
) -> Result<T, romforge_core::Error> {
    let _open = detached.read().unwrap_or_else(|e| e.into_inner());
    let at = {
        let lib = read.lock().unwrap_or_else(|e| e.into_inner());
        lib.path().map(Path::to_owned)
    }
    .unwrap_or_else(db_path);
    f(&at)
}

/// `$ROMFORGE_DB`, else the RomForge folder's database, else the old
/// default ([`romforge_core::db_path`], which the launcher provider opens
/// too).
pub fn db_path() -> PathBuf {
    romforge_core::db_path()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A library on disk, as the app holds it.
    fn db() -> (tempfile::TempDir, Arc<Mutex<Library>>, Mutex<Library>, RwLock<()>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let open = || Library::open(&path).unwrap();
        let (write, read) = (Arc::new(Mutex::new(open())), Mutex::new(open()));
        (dir, write, read, RwLock::new(()))
    }

    fn show_empty_media(lib: &mut Library) -> Result<(), romforge_core::Error> {
        let mut s = lib.settings()?;
        s.show_empty_media = true;
        lib.save_settings(&s).map(drop)
    }

    /// A long job (Enrich) holds `write` for its whole run, saving now and
    /// then; a quick edit goes through meanwhile, and the job sees it.
    #[test]
    fn an_edit_does_not_wait_for_a_job() {
        let (_dir, write, read, detached) = db();
        let (started, ends) = (std::sync::mpsc::channel(), std::sync::mpsc::channel::<()>());
        let job = std::thread::spawn({
            let write = write.clone();
            move || {
                let lib = write.lock().unwrap();
                started.0.send(()).unwrap();
                ends.1.recv().unwrap();
                lib.settings().unwrap().show_empty_media
            }
        });
        started.1.recv().unwrap();
        apart(&read, &detached, |at| Library::edit(at, show_empty_media)).unwrap();
        ends.0.send(()).unwrap();
        assert!(job.join().unwrap(), "the job sees the edit");
    }

    /// A scan holds SQLite's write lock for its whole run: the edit says
    /// so (`busy:`, which the window queues behind the job) instead of
    /// "database is locked", and goes through once the scan ends.
    #[test]
    fn an_edit_beside_a_scan_says_busy() {
        let (_dir, write, read, detached) = db();
        let (started, ends) = (std::sync::mpsc::channel(), std::sync::mpsc::channel::<()>());
        let roms = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/roms");
        let scan = std::thread::spawn({
            let write = write.clone();
            move || {
                let mut lib = write.lock().unwrap();
                let mut first = true;
                let opts = romforge_core::ScanOptions::default();
                lib.scan_with(&roms, romforge_core::Scope::All, &opts, |_| {
                    if std::mem::take(&mut first) {
                        started.0.send(()).unwrap();
                        ends.1.recv().unwrap();
                    }
                })
                .unwrap();
            }
        });
        started.1.recv().unwrap();
        let err = apart(&read, &detached, |at| Library::edit(at, show_empty_media)).unwrap_err();
        assert!(err.to_string().starts_with("busy:"), "{err}");
        ends.0.send(()).unwrap();
        scan.join().unwrap();
        apart(&read, &detached, |at| Library::edit(at, show_empty_media)).unwrap();
    }

    /// A move during a long job says so after a moment instead of waiting
    /// for it, and edits meanwhile never wait on the move.
    #[test]
    fn a_move_never_queues_behind_a_job() {
        let (_dir, write, read, detached) = db();
        let job = write.lock().unwrap();
        let wait = Duration::from_millis(300);
        let mover = std::thread::scope(|s| {
            let mover = s.spawn(|| alone(&write, &read, &detached, wait).map(drop));
            // An edit during the move's wait goes through at once.
            let t = Instant::now();
            apart(&read, &detached, |at| Library::edit(at, show_empty_media)).unwrap();
            assert!(t.elapsed() < wait, "the edit waited on the move");
            mover.join().unwrap()
        });
        let err = mover.unwrap_err().to_string();
        assert!(err.contains("a job is running"), "{err}");
        drop(job);
        assert!(
            alone(&write, &read, &detached, wait).is_ok(),
            "free once the job ends"
        );
    }

    /// A download in the folder (holding `detached`) is waited for, then
    /// named; it is never moved out from under.
    #[test]
    fn a_move_waits_for_a_download() {
        let (_dir, write, read, detached) = db();
        let download = detached.read().unwrap();
        let err = alone(&write, &read, &detached, Duration::from_millis(100))
            .map(drop)
            .unwrap_err()
            .to_string();
        assert!(err.contains("download"), "{err}");
        drop(download);
        assert!(alone(&write, &read, &detached, Duration::ZERO).is_ok());
    }
}
