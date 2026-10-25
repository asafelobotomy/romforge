//! Emulator Settings. Core downloads, installs and the core list's
//! refresh run on a connection of their own ([`read_net`]): they take
//! minutes, and `read` serves the library list meanwhile.

use romforge_core::emusettings::{EmulatorSettings, EmulatorSettingsView};

use crate::db::{Db, Res, edit, read, read_net, with};

#[tauri::command]
pub async fn get_emulator_settings(db: tauri::State<'_, Db>) -> Res<EmulatorSettingsView> {
    read(&db, move |lib| lib.emulator_settings()).await
}

#[tauri::command]
pub async fn save_emulator_settings(
    db: tauri::State<'_, Db>,
    settings: EmulatorSettings,
) -> Res<EmulatorSettings> {
    edit(&db, move |lib| lib.save_emulator_settings(&settings)).await
}

#[tauri::command]
pub async fn import_emulator_setup(
    db: tauri::State<'_, Db>,
    emulator: String,
) -> Res<romforge_core::launch::existing::SetupImported> {
    with(&db, move |lib| lib.import_emulator_setup(&emulator)).await
}

/// What moving an emulator's game data into RomForge would do.
#[tauri::command]
pub async fn move_data_plan(
    db: tauri::State<'_, Db>,
    emulator: String,
) -> Res<romforge_core::launch::move_in::MovePlan> {
    read(&db, move |lib| lib.move_data_plan(&emulator)).await
}

/// Move it in (originals to the trash).
#[tauri::command]
pub async fn move_data(db: tauri::State<'_, Db>, emulator: String) -> Res<i64> {
    with(&db, move |lib| lib.move_data(&emulator)).await
}

/// Take the last move back.
#[tauri::command]
pub async fn undo_move_data(db: tauri::State<'_, Db>, emulator: String) -> Res<usize> {
    with(&db, move |lib| lib.undo_move_data(&emulator)).await
}

/// What gathering an emulator's saves into Saves/ would do.
#[tauri::command]
pub async fn gather_saves_plan(
    db: tauri::State<'_, Db>,
    emulator: String,
) -> Res<romforge_core::saves::gather::GatherPlan> {
    read(&db, move |lib| lib.gather_saves_plan(&emulator)).await
}

/// Gather them: (gathering id, files moved).
#[tauri::command]
pub async fn gather_saves(db: tauri::State<'_, Db>, emulator: String) -> Res<(i64, usize)> {
    with(&db, move |lib| lib.gather_saves(&emulator)).await
}

/// Put the last gathering back.
#[tauri::command]
pub async fn undo_gather_saves(db: tauri::State<'_, Db>, emulator: String) -> Res<usize> {
    with(&db, move |lib| lib.undo_gather_saves(&emulator)).await
}

#[tauri::command]
pub async fn undo_emulator_setup(db: tauri::State<'_, Db>, emulator: String) -> Res<usize> {
    with(&db, move |lib| lib.undo_emulator_setup(&emulator)).await
}

#[tauri::command]
pub async fn emulator_cores(db: tauri::State<'_, Db>, refresh: bool) -> Res<romforge_core::obtain::CoreList> {
    read_net(&db, move |lib| {
        if refresh {
            lib.refresh_emulator_cores()?;
        }
        lib.emulator_cores()
    })
    .await
}

/// `accept`: the user agreed to a non-commercial licence (the core
/// refuses one without it).
#[tauri::command]
pub async fn download_core(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    core: String,
    accept: Option<bool>,
    job: Option<u64>,
) -> Res<String> {
    let job = crate::jobs::Job::start(&app, job);
    let accept = accept.unwrap_or(false);
    read_net(&db, move |lib| {
        job.watch(&core, || lib.download_core(&core, accept))
    })
    .await
}

#[tauri::command]
pub async fn remove_core(db: tauri::State<'_, Db>, core: String) -> Res<()> {
    read_net(&db, move |lib| lib.remove_core(&core)).await
}

/// Download an emulator's official AppImage into RomForge/Emulators/.
#[tauri::command]
pub async fn install_appimage(db: tauri::State<'_, Db>, emulator: String) -> Res<String> {
    read_net(&db, move |lib| {
        lib.install_appimage(&emulator).map(|p| p.display().to_string())
    })
    .await
}

#[tauri::command]
pub async fn install_emulator(db: tauri::State<'_, Db>, emulator: String) -> Res<String> {
    read_net(&db, move |lib| lib.install_emulator(&emulator)).await
}

/// All settings: every key in romforge's copy of an emulator's config.
#[tauri::command]
pub async fn raw_settings(
    db: tauri::State<'_, Db>,
    emulator: String,
) -> Res<romforge_core::emusettings::raw::RawView> {
    read(&db, move |lib| lib.raw_settings(&emulator)).await
}

#[tauri::command]
pub async fn edit_raw_settings(
    db: tauri::State<'_, Db>,
    emulator: String,
    edits: Vec<romforge_core::emusettings::raw::RawEdit>,
) -> Res<i64> {
    edit(&db, move |lib| lib.edit_raw_settings(&emulator, &edits)).await
}

#[tauri::command]
pub async fn undo_raw_settings(db: tauri::State<'_, Db>, emulator: String) -> Res<usize> {
    edit(&db, move |lib| lib.undo_raw_settings(&emulator)).await
}

/// MAME's `-createconfig` into romforge's `mame.ini`.
#[tauri::command]
pub async fn mame_write_defaults(db: tauri::State<'_, Db>) -> Res<String> {
    read_net(&db, move |lib| {
        lib.mame_write_defaults().map(|p| p.display().to_string())
    })
    .await
}

/// Settings for one game.
#[tauri::command]
pub async fn game_settings(
    db: tauri::State<'_, Db>,
    system: String,
    title: String,
) -> Res<romforge_core::emusettings::GameSettingsView> {
    read(&db, move |lib| lib.game_settings(&system, &title)).await
}

#[tauri::command]
pub async fn set_game_settings(
    db: tauri::State<'_, Db>,
    system: String,
    title: String,
    values: romforge_core::emusettings::Values,
) -> Res<romforge_core::emusettings::Values> {
    edit(&db, move |lib| lib.set_game_settings(&system, &title, &values)).await
}
