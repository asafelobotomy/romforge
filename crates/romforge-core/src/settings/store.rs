use super::*;

pub fn load(conn: &Connection) -> Result<Settings, Error> {
    let get = |key: &str| -> Result<Option<String>, Error> {
        Ok(conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    };
    Ok(Settings {
        source_dir: get("source_dir")?.map(PathBuf::from),
        library_dir: get("library_dir")?.map(PathBuf::from),
        keep_originals: get("keep_originals")?.as_deref() == Some("1"),
        in_place: get("in_place")?.as_deref() == Some("1"),
        staging_dir: get("staging_dir")?.map(PathBuf::from),
        keys: Keys {
            screenscraper_dev_id: get("screenscraper_dev_id")?,
            screenscraper_dev_password: get("screenscraper_dev_password")?,
            screenscraper_user: get("screenscraper_user")?,
            screenscraper_password: get("screenscraper_password")?,
            steamgriddb_key: get("steamgriddb_key")?,
            igdb_client_id: get("igdb_client_id")?,
            igdb_client_secret: get("igdb_client_secret")?,
        },
        show_empty_media: get("show_empty_media")?.as_deref() == Some("1"),
        platform_icons: get("platform_icons")?.unwrap_or_default(),
        logo: get("logo")?.unwrap_or_default(),
        regions: get("regions")?
            .map(|v| {
                v.split(',')
                    .map(|r| r.trim().to_owned())
                    .filter(|r| !r.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        emulator_paths: {
            let mut stmt = conn.prepare("SELECT key, value FROM settings WHERE key LIKE ?1")?;
            let rows = stmt.query_map([format!("{EMULATOR_PATH}%")], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            rows.filter_map(|r| r.ok())
                .filter_map(|(k, v)| Some((k.strip_prefix(EMULATOR_PATH)?.to_owned(), PathBuf::from(v))))
                .collect()
        },
    })
}

/// Settings keys of [`Settings::emulator_paths`]: this and the name.
const EMULATOR_PATH: &str = "emulator_path:";

/// Validate and store. Returns what was stored (absolute paths).
pub fn save(conn: &mut Connection, s: &Settings) -> Result<Settings, Error> {
    save_in(conn, s, false)
}

/// [`save`] for a library with a RomForge folder (`home`) or without:
/// imported games go to that folder, so "keep originals" needs no Library
/// folder there.
pub fn save_in(conn: &mut Connection, s: &Settings, home: bool) -> Result<Settings, Error> {
    let s = if home && s.keep_originals {
        Settings {
            keep_originals: false,
            ..s.clone()
        }
        .validated()
        .map(|v| Settings {
            keep_originals: true,
            ..v
        })?
    } else {
        s.validated()?
    };
    let tx = conn.transaction()?;
    {
        let mut put = tx.prepare(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )?;
        let mut del = tx.prepare("DELETE FROM settings WHERE key = ?1")?;
        let path = |p: &Option<PathBuf>| p.as_ref().map(|p| p.to_string_lossy().into_owned());
        for (key, value) in [
            ("source_dir", path(&s.source_dir)),
            ("library_dir", path(&s.library_dir)),
            ("staging_dir", path(&s.staging_dir)),
            (
                "keep_originals",
                Some(if s.keep_originals { "1" } else { "0" }.to_owned()),
            ),
            ("in_place", Some(if s.in_place { "1" } else { "0" }.to_owned())),
            (
                "show_empty_media",
                Some(if s.show_empty_media { "1" } else { "0" }.to_owned()),
            ),
            ("regions", Some(s.regions.join(", ")).filter(|r| !r.is_empty())),
            (
                "platform_icons",
                Some(s.platform_icons.clone()).filter(|t| !t.is_empty()),
            ),
            ("logo", Some(s.logo.clone()).filter(|l| !l.is_empty())),
        ]
        .into_iter()
        .chain(s.keys.fields())
        {
            match value {
                Some(v) => put.execute(params![key, v])?,
                None => del.execute([key])?,
            };
        }
        tx.execute(
            "DELETE FROM settings WHERE key LIKE ?1",
            [format!("{EMULATOR_PATH}%")],
        )?;
        for (name, path) in &s.emulator_paths {
            put.execute(params![format!("{EMULATOR_PATH}{name}"), path.to_string_lossy()])?;
        }
    }
    tx.commit()?;
    Ok(s)
}
