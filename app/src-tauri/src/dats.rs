//! DATs, the third-party databases and ScummVM's list.

use std::path::PathBuf;

use romforge_core::extdb::{ExtDb, ExtDbInfo};

use crate::db::{Db, Res, read, with};

#[derive(serde::Serialize)]
pub struct Downloaded {
    imported: usize,
    unchanged: usize,
    errors: Vec<String>,
}

pub async fn extdb_dir(db: &tauri::State<'_, Db>) -> Res<PathBuf> {
    read(db, |lib| Ok(lib.extdb_dir()))
        .await?
        .ok_or_else(|| "no folder for databases".into())
}

fn extdb_named(name: &str) -> Res<ExtDb> {
    ExtDb::parse(name).ok_or_else(|| format!("unknown database {name:?}"))
}

#[tauri::command]
pub async fn extdb_list(db: tauri::State<'_, Db>) -> Res<Vec<ExtDbInfo>> {
    Ok(romforge_core::extdb::list(&extdb_dir(&db).await?))
}

/// Downloads without holding the library, so the window stays usable.
#[tauri::command]
pub async fn extdb_download(db: tauri::State<'_, Db>, name: String) -> Res<ExtDbInfo> {
    let dir = extdb_dir(&db).await?;
    romforge_core::extdb::download(extdb_named(&name)?, &dir, &Default::default(), |_| {})
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn extdb_remove(db: tauri::State<'_, Db>, name: String) -> Res<()> {
    let dir = extdb_dir(&db).await?;
    romforge_core::extdb::remove(extdb_named(&name)?, &dir).map_err(|e| e.to_string())
}

/// The installed MAME's own list as the Arcade DAT (`mame -listxml`).
#[tauri::command]
pub async fn mame_dat(db: tauri::State<'_, Db>) -> Res<romforge_core::ImportedDat> {
    with(&db, move |lib| {
        lib.mame_dat(&romforge_core::launch::Env::detect())
    })
    .await
}

#[tauri::command]
pub async fn download_dats(db: tauri::State<'_, Db>, extras: bool) -> Res<Downloaded> {
    with(&db, move |lib| {
        let mut out = Downloaded {
            imported: 0,
            unchanged: 0,
            errors: Vec::new(),
        };
        lib.download_all(extras, |key, source, r| match r {
            Ok(dats) => {
                for d in dats {
                    if d.unchanged {
                        out.unchanged += 1;
                    } else {
                        out.imported += 1;
                    }
                }
            }
            Err(e) => out.errors.push(format!("{key}: {source}: {e}")),
        })?;
        Ok(out)
    })
    .await
}

/// Read ScummVM's detection list from the installed ScummVM, or from
/// `file` (one saved with `scummvm --dump-all-detection-entries`).
#[tauri::command]
pub async fn scummvm_list(db: tauri::State<'_, Db>, file: Option<String>) -> Res<romforge_core::ScummvmList> {
    with(&db, move |lib| {
        // libretro's names (its box art goes by them); without them,
        // ScummVM's own names stay.
        let _ = lib.scummvm_titles_download();
        match file {
            Some(f) => lib.scummvm_import(&std::fs::read(f)?),
            None => lib.scummvm_read(&romforge_core::launch::Env::detect()),
        }
    })
    .await
}
