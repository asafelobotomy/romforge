//! DATs, the third-party databases and ScummVM's list.

use std::path::PathBuf;
use std::sync::Mutex;

use romforge_core::extdb::{ExtDb, ExtDbInfo};
use romforge_core::{DatStatus, ImportedDat, Library};

use crate::db::{Db, Res, in_folder, read_net, with, write_net};
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

/// `systems`: only these platforms (their keys; Advanced), else every
/// platform's.
#[tauri::command]
pub async fn download_dats(
    app: tauri::AppHandle,
    db: tauri::State<'_, Db>,
    extras: bool,
    systems: Option<Vec<String>>,
    job: Option<u64>,
) -> Res<Downloaded> {
    let mut job = Job::start(&app, job);
    with(&db, move |lib| {
        let mut out = Downloaded {
            imported: 0,
            unchanged: 0,
            errors: Vec::new(),
        };
        let mut count = |key: &str,
                         source: romforge_core::fetch::Source,
                         r: Result<Vec<ImportedDat>, romforge_core::Error>| match r {
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
        };
        match systems.filter(|s| !s.is_empty()) {
            Some(keys) => {
                // A key that isn't a platform is refused, not passed over.
                let keys = keys
                    .iter()
                    .map(|k| {
                        romforge_core::systems::by_key(k)
                            .map(|s| s.key)
                            .ok_or_else(|| romforge_core::Error::UnknownSystem(k.clone()))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let cancel = job.cancel.clone();
                let progress = |n, total| job.count(n, total, format!("{n} of {total} DATs"));
                lib.download_systems_with(&keys, extras, &cancel, progress, |key, source, r| {
                    count(key, source, r)
                })?;
            }
            None => {
                let cancel = job.cancel.clone();
                let progress = |n, total| job.count(n, total, format!("{n} of {total} DATs"));
                lib.download_all_with(extras, &cancel, progress, count)?;
            }
        }
        Ok(out)
    })
    .await
}

/// Import DAT files (or zips of them) from disk: `system` is for a file
/// that doesn't say which platform it is for. Advanced; the CLI's
/// `dat import`.
#[tauri::command]
pub async fn import_dat_files(
    db: tauri::State<'_, Db>,
    files: Vec<String>,
    system: Option<String>,
) -> Res<Vec<ImportedDat>> {
    with(&db, move |lib| {
        lib.batch(|lib| {
            let mut all = Vec::new();
            for file in &files {
                all.extend(lib.import_dats(file, system.as_deref())?);
            }
            Ok::<_, romforge_core::Error>(all)
        })
    })
    .await
}

/// Which of No-Intro's DATs for the platforms romforge knows are missing
/// or outdated here (it asks No-Intro for its list). The CLI's `dat check`.
#[tauri::command]
pub async fn nointro_status(db: tauri::State<'_, Db>) -> Res<Vec<DatStatus>> {
    read_net(&db, move |lib| {
        let profile = romforge_core::fetch::nointro_profile()?;
        lib.nointro_status(&profile)
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
