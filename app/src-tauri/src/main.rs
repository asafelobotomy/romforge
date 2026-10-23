//! Tauri shell over romforge-core. Every command is a call into the core.
//! Those that touch the database, files or the network run on a blocking
//! thread (never the main thread or an async worker); the few left
//! synchronous only read memory (`job_cancel`, `now_playing`...). Commands
//! that only read use a second connection: SQLite's WAL lets them go on
//! while a scan, Enrich or a download writes. Reads that wait on the
//! network open their own (`db::read_net`). The `art://`, `theme://` and
//! `music://` schemes answer on a few worker threads ([`ArtPool`]).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod art;
mod cleanup;
mod dats;
mod db;
mod emulators;
mod enrich;
mod firmware;
mod home;
mod hotkeys;
mod jobs;
mod library;
mod manual;
mod marks;
mod music;
mod overlay;
mod patch;
mod play;
mod scan;
mod search;
mod settings;
mod start;
mod steam;
mod themes;
mod tray;

use std::sync::{Arc, Mutex, RwLock};

use romforge_core::Library;

use crate::art::{ArtPool, ArtRoots, serve_art};
use crate::db::{Db, db_path};

fn main() {
    let path = db_path();
    // A library that can't open is said in a dialog, then romforge ends
    // (`setup`); meanwhile empty ones in memory stand in.
    let opened = Library::open(&path).and_then(|w| Ok((w, Library::open(&path)?)));
    let (write, read, failed) = match opened {
        Ok((w, r)) => (w, r, None),
        Err(e) => {
            let stand_in = || Library::open_in_memory().expect("a library in memory");
            let why = format!("romforge couldn't open its library {}:\n\n{e}", path.display());
            eprintln!("romforge: {why}");
            (stand_in(), stand_in(), Some(why))
        }
    };
    let db = Db {
        write: Arc::new(Mutex::new(write)),
        read: Arc::new(Mutex::new(read)),
        detached: Arc::new(RwLock::new(())),
    };
    let pool = ArtPool::new();
    let roots = {
        let lib = db.read.lock().unwrap_or_else(|e| e.into_inner());
        themes::set_dir(&lib);
        music::set_dir(lib.settings().ok().and_then(|s| s.music_dir));
        ArtRoots(Arc::new(RwLock::new(ArtRoots::of(&lib))))
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("art", {
            let (roots, pool) = (roots.clone(), pool.clone());
            move |_ctx, request, responder| {
                let roots = roots.0.read().unwrap_or_else(|e| e.into_inner()).clone();
                let path = request.uri().path().to_owned();
                let query = request.uri().query().map(str::to_owned);
                pool.run(move || responder.respond(serve_art(&roots, &path, query.as_deref())));
            }
        })
        .register_asynchronous_uri_scheme_protocol("theme", {
            let pool = pool.clone();
            move |_ctx, request, responder| {
                let path = request.uri().path().to_owned();
                pool.run(move || responder.respond(themes::serve(&path)));
            }
        })
        .register_asynchronous_uri_scheme_protocol("music", move |_ctx, request, responder| {
            pool.run(move || responder.respond(music::serve(&request)));
        })
        .manage(db)
        .manage(roots)
        .manage(play::Now::default())
        .manage(jobs::Jobs::default())
        .manage(hotkeys::Watcher::default())
        .manage(start::start_options(std::env::args().skip(1)))
        // Game pads, heard even while an emulator has the focus: "pad"
        // events for the page (no pads, or no access: the app works without).
        .setup(move |app| {
            if let Some(why) = failed {
                use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
                let handle = app.handle().clone();
                app.dialog()
                    .message(why)
                    .title("romforge")
                    .kind(MessageDialogKind::Error)
                    .show(move |_| handle.exit(1));
                return Ok(());
            }
            let handle = app.handle().clone();
            let listening = romforge_core::controller::listen(move |e| {
                // Raw buttons, the stick and time are for the hotkeys; the
                // page gets the actions and pads coming and going.
                use romforge_core::controller::PadEvent;
                match e {
                    PadEvent::Button { .. } | PadEvent::Stick { .. } | PadEvent::Tick => {
                        hotkeys::pad(&handle, &e)
                    }
                    // A pad going lets go of what it held, then the page hears.
                    PadEvent::Disconnected { .. } => {
                        hotkeys::pad(&handle, &e);
                        let _ = tauri::Emitter::emit(&handle, "pad", &e);
                    }
                    _ => {
                        let _ = tauri::Emitter::emit(&handle, "pad", &e);
                    }
                }
            });
            if let Err(e) = listening {
                eprintln!("romforge: {e}");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            library::summary,
            library::dat_games,
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
            enrich::platform_logo,
            scan::can_refresh,
            cleanup::convert_plan,
            cleanup::convert_apply,
            home::home_info,
            home::home_create,
            home::home_move,
            enrich::enrich,
            play::launch,
            manual::manual_info,
            manual::manual_lookup,
            manual::add_manual,
            manual::download_manual,
            manual::manual_offers,
            manual::download_manual_offer,
            manual::remove_manual,
            manual::set_manual_page,
            manual::manual_marks,
            manual::add_manual_mark,
            manual::edit_manual_mark,
            manual::remove_manual_mark,
            manual::manual_ocr,
            manual::save_manual_ocr,
            manual::manual_ocr_texts,
            manual::ocr_languages,
            manual::ocr_download,
            play::stop_game,
            play::now_playing,
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
            emulators::emulator_cores,
            emulators::download_core,
            emulators::remove_core,
            emulators::install_emulator,
            settings::get_settings,
            settings::save_settings,
            settings::set_fs_details,
            settings::set_fs_bookshelf,
            settings::settings_info,
            dats::scummvm_list,
            search::search_queue,
            search::search_accept,
            search::search_find,
            search::search_preview,
            search::search_apply,
            search::search_fetch,
            jobs::job_cancel,
            tray::tuck_away,
            hotkeys::hotkeys_view,
            themes::themes_view,
            themes::theme_set,
            themes::theme_stamp,
            themes::themes_folder,
            themes::themes_available,
            marks::marks,
            marks::set_favourite,
            marks::game_emulator,
            marks::set_game_emulator,
            music::music_tracks,
            steam::steam_status,
            steam::steam_plan,
            steam::steam_add,
            steam::steam_remove,
            themes::theme_install,
            themes::theme_remove,
            themes::theme_preview,
            start::start_options_now,
            start::quit,
            search::forget_identity,
            library::all_systems
        ])
        .run(tauri::generate_context!())
        .expect("error while running romforge");
}
