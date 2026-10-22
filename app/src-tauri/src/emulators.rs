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

#[tauri::command]
pub async fn install_emulator(db: tauri::State<'_, Db>, emulator: String) -> Res<String> {
    read_net(&db, move |lib| lib.install_emulator(&emulator)).await
}
