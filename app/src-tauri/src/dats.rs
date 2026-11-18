//! DATs, the third-party databases and ScummVM's list.

use std::path::PathBuf;
use std::sync::Mutex;

use romforge_core::Library;
use romforge_core::extdb::{ExtDb, ExtDbInfo};

use crate::db::{Db, Res, in_folder, with, write_net};
use crate::jobs::Job;

#[derive(serde::Serialize)]
pub struct Downloaded {
    imported: usize,
    unchanged: usize,
    errors: Vec<String>,
}

fn extdb_named(name: &str) -> Res<ExtDb> {
    ExtDb::parse(name).ok_or_else(|| format!("unknown database {name:?}"))
}

/// Where the downloaded databases are, looked up under [`in_folder`]
/// (so a move of the RomForge folder can't change it meanwhile).
fn extdb_dir(read: &Mutex<Library>) -> Result<PathBuf, romforge_core::Error> {
    read.lock()
        .unwrap_or_else(|e| e.into_inner())
        .extdb_dir()
        .ok_or_else(|| romforge_core::Error::Settings("no folder for databases".into()))
}

#[tauri::command]
pub async fn extdb_list(db: tauri::State<'_, Db>) -> Res<Vec<ExtDbInfo>> {
    in_folder(&db, |read| Ok(romforge_core::extdb::list(&extdb_dir(read)?))).await
}

/// Downloads without holding the library, so the window stays usable; a
/// move of the RomForge folder waits for it.
#[tauri::command]
pub async fn extdb_download(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    name: String,
    job: Option<u64>,
) -> Res<ExtDbInfo> {
    let which = extdb_named(&name)?;
    let job = Job::start(&app, job);
    in_folder(&db, move |read| {
        let dir = extdb_dir(read)?;
        job.watch(&name, || {
            romforge_core::extdb::download(which, &dir, &Default::default(), |_| {})
        })
    })
    .await
}

#[tauri::command]
pub async fn extdb_remove(db: tauri::State<'_, Db>, name: String) -> Res<()> {
    let which = extdb_named(&name)?;
    in_folder(&db, move |read| {
        romforge_core::extdb::remove(which, &extdb_dir(read)?)
    })
    .await
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
        let bytes = file.map(std::fs::read).transpose()?;
        lib.scummvm_refresh(bytes.as_deref())
    })
    .await
}
