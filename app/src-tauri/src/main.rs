//! Tauri shell over romforge-core. Every command is a call into the core.
//! Each runs on a blocking thread (never the main thread or an async
//! worker), and commands that only read use a second connection: SQLite's
//! WAL lets them go on while a scan, Enrich or a download writes. Reads
//! that wait on the network open their own (`db::read_net`).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod art;
mod cleanup;
mod dats;
mod db;
mod emulators;
mod enrich;
mod firmware;
mod home;
mod images;
mod library;
mod patch;
mod scan;
mod search;
mod settings;

use std::sync::{Arc, Mutex, RwLock};

use romforge_core::Library;

use crate::art::{ArtPool, ArtRoots, serve_art};
use crate::db::{Db, db_path};

fn main() {
    let path = db_path();
    let open = || Library::open(&path).unwrap_or_else(|e| panic!("opening {}: {e}", path.display()));
    let db = Db {
        write: Arc::new(Mutex::new(open())),
        read: Arc::new(Mutex::new(open())),
    };
    let roots = {
        let lib = db.read.lock().unwrap_or_else(|e| e.into_inner());
        ArtRoots(Arc::new(RwLock::new(ArtRoots::of(&lib))))
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("art", {
            let roots = roots.clone();
            let pool = ArtPool::new();
            move |_ctx, request, responder| {
                let roots = roots.0.read().unwrap_or_else(|e| e.into_inner()).clone();
                let path = request.uri().path().to_owned();
                let query = request.uri().query().map(str::to_owned);
                pool.run(move || responder.respond(serve_art(&roots, &path, query.as_deref())));
            }
        })
        .manage(db)
        .manage(roots)
        .invoke_handler(tauri::generate_handler![
            library::summary,
            library::dat_games,
            scan::scan,
            dats::download_dats,
            dats::mame_dat,
            dats::extdb_list,
            dats::extdb_download,
            dats::extdb_remove,
            cleanup::organize_plan,
            cleanup::organize_apply,
            cleanup::organize_undo,
            cleanup::cleanup_plan,
            cleanup::cleanup_apply,
            cleanup::cleanup_undo,
            cleanup::cleanup_batches,
            cleanup::trash_contents,
            cleanup::empty_trash,
            patch::patch_plan,
            patch::patch_apply,
            patch::patches_beside,
            enrich::art_choices,
            enrich::choose_art,
            scan::scan_folder,
            scan::scan_preview,
            enrich::platform_icons,
            scan::can_refresh,
            cleanup::convert_plan,
            cleanup::convert_apply,
            home::home_info,
            home::home_create,
            home::home_move,
            enrich::enrich,
            library::launch,
            library::library_view,
            library::game_detail,
            scan::refresh,
            emulators::get_emulator_settings,
            firmware::firmware_report,
            firmware::firmware_download,
            firmware::firmware_import_plan,
            firmware::firmware_import,
            firmware::firmware_undo,
            firmware::install_ps3_firmware,
            emulators::save_emulator_settings,
            emulators::import_emulator_setup,
            emulators::undo_emulator_setup,
            settings::get_settings,
            settings::save_settings,
            settings::settings_info,
            dats::scummvm_list,
            search::search_queue,
            search::search_find,
            search::search_preview,
            search::search_choose,
            search::forget_identity,
            library::all_systems
        ])
        .run(tauri::generate_context!())
        .expect("error while running romforge");
}
