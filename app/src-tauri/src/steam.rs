//! romforge in Steam (stage 2, step 7): Settings → Steam's commands
//! (romforge_core::steam). Nothing is written while Steam runs.

use romforge_core::steam::{self, Status, SteamPlan};

use crate::db::{Res, blocking};

fn root() -> Result<std::path::PathBuf, romforge_core::Error> {
    steam::roots()
        .into_iter()
        .next()
        .ok_or_else(|| romforge_core::Error::Settings("Steam isn't installed here".into()))
}

#[tauri::command]
pub async fn steam_status() -> Res<Status> {
    blocking(|| Ok(steam::status())).await
}

/// What adding romforge for `account` would write (the dry run).
#[tauri::command]
pub async fn steam_plan(account: String) -> Res<SteamPlan> {
    blocking(move || steam::plan(&root()?, &account, &steam::this_exe()?)).await
}

#[tauri::command]
pub async fn steam_add(account: String) -> Res<()> {
    blocking(move || steam::apply(&steam::plan(&root()?, &account, &steam::this_exe()?)?)).await
}

#[tauri::command]
pub async fn steam_remove(account: String) -> Res<bool> {
    blocking(move || steam::remove(&root()?, &account)).await
}
