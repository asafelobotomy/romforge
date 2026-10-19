use super::*;

/// The local copy of a database, if downloaded, opened read-only.
pub fn open(db: ExtDb, dir: &Path) -> Option<Connection> {
    let path = dir.join(db.file());
    if !path.exists() {
        return None;
    }
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()
}

pub fn info(db: ExtDb, dir: &Path) -> ExtDbInfo {
    let path = dir.join(db.file());
    let conn = open(db, dir);
    let get = |key: &str| -> Option<String> {
        conn.as_ref()?
            .query_row("SELECT value FROM romforge_meta WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .ok()?
    };
    ExtDbInfo {
        db,
        name: db.name(),
        about: db.about(),
        homepage: db.homepage(),
        downloaded_at: get("downloaded_at"),
        bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
        entries: get("entries").and_then(|v| v.parse().ok()),
    }
}

pub fn list(dir: &Path) -> Vec<ExtDbInfo> {
    ExtDb::ALL.into_iter().map(|d| info(d, dir)).collect()
}

/// Delete the local copy.
pub fn remove(db: ExtDb, dir: &Path) -> Result<(), Error> {
    match std::fs::remove_file(dir.join(db.file())) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Download (or update) a database and build its local copy. The download
/// replaces the old copy only once the new one is complete.
pub fn download(
    db: ExtDb,
    dir: &Path,
    bases: &Bases,
    mut progress: impl FnMut(&str),
) -> Result<ExtDbInfo, Error> {
    std::fs::create_dir_all(dir)?;
    let zip = dir.join(format!("{}.zip", db.key()));
    let url = match db {
        ExtDb::OpenVgdb => &bases.openvgdb,
        ExtDb::LaunchBox => &bases.launchbox,
    };
    let mut last = 0;
    // LaunchBox's Metadata.zip is about 110 MB; this only stops a runaway.
    fetch::download_to(url, &zip, 2 << 30, |done, total| {
        if done - last >= 8 << 20 || Some(done) == total {
            last = done;
            progress(&match total {
                Some(t) => format!("downloading {} MB of {} MB", done >> 20, t >> 20),
                None => format!("downloading {} MB", done >> 20),
            });
        }
    })?;
    let result = import(db, &zip, dir, url, progress);
    let _ = std::fs::remove_file(&zip);
    result
}

/// Build the local copy from a zip already on disk (a download, or one the
/// user fetched by hand). `source` is noted as where it came from.
pub fn import(
    db: ExtDb,
    zip: &Path,
    dir: &Path,
    source: &str,
    mut progress: impl FnMut(&str),
) -> Result<ExtDbInfo, Error> {
    std::fs::create_dir_all(dir)?;
    let part = dir.join(format!("{}.part", db.file()));
    let _ = std::fs::remove_file(&part);
    let built = match db {
        ExtDb::OpenVgdb => build_openvgdb(zip, &part),
        ExtDb::LaunchBox => build_launchbox(zip, &part, &mut progress),
    };
    let entries = match built {
        Ok(n) => n,
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
    };
    {
        let conn = Connection::open(&part)?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS romforge_meta (key TEXT PRIMARY KEY, value TEXT)")?;
        for (k, v) in [
            ("downloaded_at", now()),
            ("entries", entries.to_string()),
            ("source", source.to_owned()),
        ] {
            conn.execute(
                "INSERT OR REPLACE INTO romforge_meta (key, value) VALUES (?1, ?2)",
                params![k, v],
            )?;
        }
    }
    std::fs::rename(&part, dir.join(db.file()))?;
    Ok(info(db, dir))
}

fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    Connection::open_in_memory()
        .and_then(|c| {
            c.query_row(
                "SELECT strftime('%Y-%m-%dT%H:%M:%SZ', ?1, 'unixepoch')",
                [secs],
                |r| r.get(0),
            )
        })
        .unwrap_or_default()
}
