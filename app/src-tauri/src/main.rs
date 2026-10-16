//! Tauri shell: read-only views over the romforge database. All logic is in
//! romforge-core; scanning and DAT import are done with romforge-cli for now.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;

use romforge_core::{DatGame, Library, LibraryFile, LibraryFilter, Status, Summary, SystemInfo};

struct Db(Mutex<Library>);

type Res<T> = Result<T, String>;

fn with<T>(db: &tauri::State<Db>, f: impl FnOnce(&Library) -> Result<T, romforge_core::Error>) -> Res<T> {
    let lib = db.0.lock().map_err(|e| e.to_string())?;
    f(&lib).map_err(|e| e.to_string())
}

#[tauri::command]
fn library(db: tauri::State<Db>, system_id: Option<i64>, status: Option<String>) -> Res<Vec<LibraryFile>> {
    let status = match status {
        Some(s) => Some(Status::parse(&s).ok_or_else(|| format!("unknown status {s:?}"))?),
        None => None,
    };
    with(&db, |lib| lib.files(&LibraryFilter { system_id, status }))
}

#[tauri::command]
fn systems(db: tauri::State<Db>) -> Res<Vec<SystemInfo>> {
    with(&db, Library::systems_in_use)
}

#[tauri::command]
fn summary(db: tauri::State<Db>) -> Res<Summary> {
    with(&db, Library::summary)
}

#[tauri::command]
fn dat_games(db: tauri::State<Db>, dat_id: i64) -> Res<Vec<DatGame>> {
    with(&db, |lib| lib.dat_games(dat_id))
}

fn main() {
    let path = std::env::var_os("ROMFORGE_DB")
        .map(PathBuf::from)
        .unwrap_or_else(romforge_core::default_db_path);
    let lib = Library::open(&path).unwrap_or_else(|e| panic!("opening {}: {e}", path.display()));
    tauri::Builder::default()
        .manage(Db(Mutex::new(lib)))
        .invoke_handler(tauri::generate_handler![library, systems, summary, dat_games])
        .run(tauri::generate_context!())
        .expect("error while running romforge");
}
