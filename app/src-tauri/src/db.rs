//! The library's two connections, and running calls on them.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use romforge_core::Library;
use romforge_core::home::Home;

/// The library twice: `write` for commands that change it, `read` for
/// those that only look (a second connection to the same database).
pub struct Db {
    pub write: Arc<Mutex<Library>>,
    pub read: Arc<Mutex<Library>>,
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

/// `$ROMFORGE_DB`, else the RomForge folder's database, else the old
/// default.
pub fn db_path() -> PathBuf {
    std::env::var_os("ROMFORGE_DB")
        .map(PathBuf::from)
        .or_else(|| Home::locate().map(|h| h.db()))
        .unwrap_or_else(romforge_core::default_db_path)
}
