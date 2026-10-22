//! The library list, a game's details, and starting a game.

use romforge_core::view::{GameDetail, LibraryView};
use romforge_core::{DatGame, Summary};

use crate::db::{Db, Res, read};

#[tauri::command]
pub async fn summary(db: tauri::State<'_, Db>) -> Res<Summary> {
    read(&db, move |lib| lib.summary()).await
}

#[tauri::command]
pub async fn dat_games(db: tauri::State<'_, Db>, dat_id: i64) -> Res<Vec<DatGame>> {
    read(&db, move |lib| lib.dat_games(dat_id)).await
}

#[derive(serde::Serialize)]
pub struct SystemChoice {
    key: &'static str,
    name: &'static str,
    /// Its splash colour (`System::accent`).
    accent: &'static str,
}

/// Every platform, for picking one by hand.
#[tauri::command]
pub async fn all_systems() -> Res<Vec<SystemChoice>> {
    let mut v: Vec<SystemChoice> = romforge_core::systems::SYSTEMS
        .iter()
        .map(|s| SystemChoice {
            key: s.key,
            name: s.name,
            accent: s.accent(),
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(b.name));
    Ok(v)
}

#[derive(serde::Serialize)]
pub struct ViewSince {
    generation: i64,
    /// `None` when the page's `known` generation is still the current one.
    view: Option<LibraryView>,
}

/// The library list, only when it changed since the generation the page
/// has (`known`): a big library's list is megabytes.
#[tauri::command]
pub async fn library_view(db: tauri::State<'_, Db>, known: Option<i64>) -> Res<ViewSince> {
    read(&db, move |lib| {
        let (generation, view) = lib.library_view_since(known)?;
        Ok(ViewSince { generation, view })
    })
    .await
}

#[tauri::command]
pub async fn game_detail(db: tauri::State<'_, Db>, id: String) -> Res<GameDetail> {
    // Pictures stay file paths: the page loads them through `art://` as
    // they're shown, instead of megabytes of base64 in this answer.
    read(&db, move |lib| lib.game_detail(&id)).await
}
