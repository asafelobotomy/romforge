use super::*;

pub(super) fn describe(r: &Runner) -> String {
    match r {
        Runner::Program(p) => p.display().to_string(),
        Runner::Flatpak { id, .. } => format!("flatpak run {id}"),
    }
}

/// A matched file of a game, or one you named in Search: (game, system key,
/// path on disk, member).
fn candidates(conn: &Connection, query: &str) -> Result<Vec<(String, String, PathBuf, String)>, Error> {
    let as_path = std::fs::canonicalize(query).ok();
    let mut stmt = conn.prepare(
        concat!("SELECT name, key, path, member FROM (
         SELECT g.name AS name, s.key AS key, f.path AS path, f.member AS member
         FROM files f JOIN matches m ON m.file_id = f.file_id AND m.out_of_scope = 0
              AND m.via IN ", crate::report::strong!(), "
         JOIN roms r ON r.rom_id = m.rom_id JOIN games g ON g.game_id = r.game_id
         JOIN dats d ON d.dat_id = g.dat_id JOIN systems s ON s.system_id = d.system_id
         WHERE f.status = 'matched' AND (f.path = ?1 OR g.name = ?2 OR (?1 IS NULL AND g.name LIKE '%' || ?3 || '%' ESCAPE '\\'))
         UNION
         SELECT mi.title, s.key, f.path, f.member
         FROM files f JOIN manual_ids mi ON mi.sha1 = f.sha1 AND mi.size = f.size
         JOIN systems s ON s.system_id = mi.system_id
         WHERE f.status = 'manual' AND (f.path = ?1 OR mi.title = ?2 OR (?1 IS NULL AND mi.title LIKE '%' || ?3 || '%' ESCAPE '\\'))
         ) ORDER BY name <> ?2, name, path, member"),
    )?;
    let rows = stmt.query_map(
        params![
            as_path.map(|p| p.to_string_lossy().into_owned()),
            query,
            crate::like_escape(query)
        ],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                PathBuf::from(r.get::<_, String>(2)?),
                r.get(3)?,
            ))
        },
    )?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Work out how to start `query` (a game name, part of one, or a file path)
/// with `emulator` (a name from [`status`]) or the best installed one.
pub fn plan(conn: &Connection, env: &Env, query: &str, emulator: Option<&str>) -> Result<LaunchSpec, Error> {
    let found = candidates(conn, query)?;
    if found.is_empty() {
        // A game that is a folder (ScummVM, DOS) by its title.
        let rows = crate::view::library(conn)?.rows;
        let q = query.to_lowercase();
        let folders: Vec<&crate::view::GameRow> = rows
            .iter()
            .filter(|r| r.folder.is_some() && r.playable)
            .filter(|r| r.title == query || r.title.to_lowercase().contains(&q))
            .collect();
        return match folders.as_slice() {
            [] => Err(Error::Launch(format!("no matched file for {query:?}"))),
            [one] => plan_row(conn, env, &one.id, emulator),
            many => match many.iter().find(|r| r.title == query) {
                Some(r) => plan_row(conn, env, &r.id, emulator),
                None => Err(Error::Launch(format!(
                    "{query:?} matches several games: {}",
                    many.iter()
                        .take(5)
                        .map(|r| r.title.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ")
                ))),
            },
        };
    };
    // A game is a title on a system: the same title on two systems is two
    // games, never one game with both's files.
    let mut games: std::collections::BTreeSet<(&str, &str)> =
        found.iter().map(|f| (f.0.as_str(), f.1.as_str())).collect();
    if games.iter().any(|g| g.0 == query) {
        games.retain(|g| g.0 == query);
    }
    let (game, system_key) = match games.iter().collect::<Vec<_>>().as_slice() {
        [one] => **one,
        many => {
            let some: Vec<String> = many.iter().take(5).map(|(t, s)| format!("{t} ({s})")).collect();
            return Err(Error::Launch(format!(
                "{query:?} matches several games: {}",
                some.join(" | ")
            )));
        }
    };
    let files: Vec<(PathBuf, String)> = found
        .iter()
        .filter(|f| f.0 == game && f.1 == system_key)
        .map(|f| (f.2.clone(), f.3.clone()))
        .collect();
    start(conn, env, game, system_key, &files, emulator)
}

/// Work out how to start one row of the library (as [`crate::view`] lists
/// it): exactly its files, whatever else shares its title.
pub fn plan_row(
    conn: &Connection,
    env: &Env,
    row_id: &str,
    emulator: Option<&str>,
) -> Result<LaunchSpec, Error> {
    let v = crate::view::library(conn)?;
    let row = v
        .rows
        .iter()
        .find(|r| r.id == row_id)
        .ok_or_else(|| Error::Launch(format!("no library row {row_id:?}")))?;
    if !row.playable {
        return Err(Error::Launch(format!(
            "{} isn't identified yet (Search), so it isn't started",
            row.title
        )));
    }
    let system = row
        .system
        .clone()
        .ok_or_else(|| Error::Launch(format!("{} has no platform", row.title)))?;
    let mut stmt = conn.prepare_cached("SELECT path, member FROM files WHERE file_id = ?1")?;
    let mut files = Vec::new();
    for id in row.file_ids() {
        let (path, member): (String, String) = stmt.query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        files.push((PathBuf::from(path), member));
    }
    files.sort();
    // An arcade game's files are in its own set's zip and its parent's and
    // BIOS's: start the one named after the game (a clone in a merged set
    // has none: then its parent's).
    if systems::by_key(&system).is_some_and(|s| s.arcade) {
        let named = |f: &(PathBuf, String)| f.0.file_stem().is_some_and(|s| s.to_string_lossy() == row.title);
        if let Some(own) = files.iter().find(|f| named(f)).cloned() {
            files.retain(|f| f.0 == own.0);
        }
    }
    if let Some(folder) = &row.folder {
        let folder = PathBuf::from(folder);
        // A DOS game in a folder starts from its program (DOSBox mounts the
        // folder around it); in an archive, from the archive. A folder of
        // its own (ScummVM) is handed over as the folder.
        let dos = systems::by_key(&system).is_some_and(|s| s.folders == systems::Folders::Zip);
        if folder.is_dir() {
            let program = files
                .iter()
                .find(|f| dos && f.1.is_empty() && crate::view::is_program(&f.0.to_string_lossy()))
                .cloned();
            files = vec![program.unwrap_or((folder, String::new()))];
        } else {
            files.retain(|f| f.0 == folder && !f.1.is_empty());
        }
    }
    // The emulator picked for this game, unless one is asked for; if it
    // has gone, romforge's own choice.
    if emulator.is_none()
        && let Some(mine) = crate::user_games::emulator(conn, &system, &row.title)?
        && let Ok(spec) = start(conn, env, &row.title, &system, &files, Some(&mine))
    {
        return Ok(spec);
    }
    start(conn, env, &row.title, &system, &files, emulator)
}
