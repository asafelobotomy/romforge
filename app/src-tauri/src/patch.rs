//! The patcher (milestone 19).

use std::path::Path;

use crate::db::{Db, Res, read, with};

/// Milestone 19: what applying a patch to a game would make.
#[tauri::command]
pub async fn patch_plan(
    db: tauri::State<'_, Db>,
    row_id: String,
    patch: String,
    hack: bool,
    tag: Option<String>,
) -> Res<romforge_core::patcher::PatchPlan> {
    read(&db, move |lib| {
        lib.patch_plan(&row_id, Path::new(&patch), Some(relation(hack)), tag.as_deref())
    })
    .await
}

/// Make the patched copy (planned again here: the plan isn't sent back).
#[tauri::command]
pub async fn patch_apply(
    db: tauri::State<'_, Db>,
    row_id: String,
    patch: String,
    hack: bool,
    tag: Option<String>,
) -> Res<romforge_core::patcher::Patched> {
    with(&db, move |lib| {
        let plan = lib.patch_plan(&row_id, Path::new(&patch), Some(relation(hack)), tag.as_deref())?;
        lib.patch_apply(&plan)
    })
    .await
}

#[tauri::command]
pub async fn patches_beside(
    db: tauri::State<'_, Db>,
    row_id: String,
) -> Res<Vec<romforge_core::patcher::FoundPatch>> {
    read(&db, move |lib| lib.patches_beside(&row_id)).await
}

fn relation(hack: bool) -> romforge_core::view::Relation {
    if hack {
        romforge_core::view::Relation::Hack
    } else {
        romforge_core::view::Relation::Translation
    }
}
