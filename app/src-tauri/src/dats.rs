//! DATs, the third-party databases and ScummVM's list.

use std::path::PathBuf;

use romforge_core::extdb::{ExtDb, ExtDbInfo};

use crate::db::{Db, Res, blocking, read, with, write_net};
use crate::jobs::Job;

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
    let dir = extdb_dir(&db).await?;
    blocking(move || Ok(romforge_core::extdb::list(&dir))).await
}

/// Downloads without holding the library, so the window stays usable.
#[tauri::command]
pub async fn extdb_download(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    name: String,
    job: Option<u64>,
) -> Res<ExtDbInfo> {
    let (dir, which) = (extdb_dir(&db).await?, extdb_named(&name)?);
    let job = Job::start(&app, job);
    blocking(move || {
        job.watch(&name, || {
            romforge_core::extdb::download(which, &dir, &Default::default(), |_| {})
        })
    })
    .await
}

#[tauri::command]
pub async fn extdb_remove(db: tauri::State<'_, Db>, name: String) -> Res<()> {
    let (dir, which) = (extdb_dir(&db).await?, extdb_named(&name)?);
    blocking(move || romforge_core::extdb::remove(which, &dir)).await
}

/// The installed MAME's own list as the Arcade DAT (`mame -listxml`).
#[tauri::command]
pub async fn mame_dat(db: tauri::State<'_, Db>) -> Res<romforge_core::ImportedDat> {
    with(&db, move |lib| {
        let env = lib.env()?;
        lib.mame_dat(&env)
    })
    .await
}

#[tauri::command]
pub async fn download_dats(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    extras: bool,
    job: Option<u64>,
) -> Res<Downloaded> {
    let mut job = Job::start(&app, job);
    with(&db, move |lib| {
        let mut out = Downloaded {
            imported: 0,
            unchanged: 0,
            errors: Vec::new(),
        };
        let cancel = job.cancel.clone();
        let progress = |n, total| job.count(n, total, format!("{n} of {total} DATs"));
        lib.download_all_with(extras, &cancel, progress, |key, source, r| match r {
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
/// `file` (one saved with `scummvm --dump-all-detection-entries`). On its
/// own connection ([`write_net`]): it downloads libretro's titles first.
#[tauri::command]
pub async fn scummvm_list(db: tauri::State<'_, Db>, file: Option<String>) -> Res<romforge_core::ScummvmList> {
    write_net(&db, move |lib| {
        // libretro's names (its box art goes by them); without them,
        // ScummVM's own names stay.
        let _ = lib.scummvm_titles_download();
        match file {
            Some(f) => lib.scummvm_import(&std::fs::read(f)?),
            None => {
                let env = lib.env()?;
                lib.scummvm_read(&env)
            }
        }
    })
    .await
}
