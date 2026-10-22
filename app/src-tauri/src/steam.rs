//! romforge in Steam (stage 2, step 7): Settings → Steam's commands
//! (romforge_core::steam). Nothing is written while Steam runs.

use romforge_core::steam::{self, Status, SteamPlan};

use crate::db::{Res, blocking};

#[tauri::command]
pub async fn steam_status() -> Res<Status> {
    blocking(|| Ok(steam::status())).await
}

/// What adding romforge for `account` would write (the dry run).
fn plan(account: &str) -> Result<SteamPlan, romforge_core::Error> {
    let (root, account) = steam::resolve(Some(account))?;
    steam::plan(&root, &account, &steam::this_exe()?)
}

#[tauri::command]
pub async fn steam_plan(account: String) -> Res<SteamPlan> {
    blocking(move || plan(&account)).await
}

#[tauri::command]
pub async fn steam_add(account: String) -> Res<()> {
    blocking(move || steam::apply(&plan(&account)?)).await
}

#[tauri::command]
pub async fn steam_remove(account: String) -> Res<bool> {
    blocking(move || {
        let (root, account) = steam::resolve(Some(&account))?;
        steam::remove(&root, &account)
    })
    .await
}
