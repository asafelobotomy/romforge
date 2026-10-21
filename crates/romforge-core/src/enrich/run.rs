use super::*;

/// (system id, game name) -> CRC32s of its ROMs, for the games you have
/// (complete, by the same rule as the have/missing report). A short audio
/// track shared by hundreds of discs makes none of them "had".
fn games_you_have(
    conn: &Connection,
    only: Option<i64>,
) -> Result<BTreeMap<(i64, String), Vec<String>>, Error> {
    use crate::report::{GAME_HAVE, HAD, IS_HAVE};
    let mut stmt = conn.prepare(&format!(
        "WITH {HAD},
              games_in AS (SELECT DISTINCT r.game_id FROM had JOIN roms r ON r.rom_id = had.rom_id),
              {GAME_HAVE}
         SELECT d.system_id, g.name, r.crc32
         FROM game_have gh JOIN games g ON g.game_id = gh.game_id
         JOIN dats d ON d.dat_id = g.dat_id
         JOIN roms r ON r.game_id = g.game_id
         WHERE {IS_HAVE} AND d.system_id IS NOT NULL AND (?1 IS NULL OR d.system_id = ?1)"
    ))?;
    let mut out: BTreeMap<(i64, String), Vec<String>> = BTreeMap::new();
    for row in stmt.query_map([only], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })? {
        let (system, game, crc) = row?;
        let crcs = out.entry((system, game)).or_default();
        if let Some(crc) = crc {
            crcs.push(crc);
        }
    }
    // Games you named in Search have no DAT ROMs, but get metadata and art
    // by title like any other (and by their files' CRC32s where a file of
    // metadata has them: DOS.dat).
    let mut named = conn.prepare(
        "SELECT DISTINCT m.system_id, m.title, f.crc32 FROM manual_ids m
         JOIN files f ON f.sha1 = m.sha1 AND f.size = m.size AND f.status = 'manual'
         WHERE ?1 IS NULL OR m.system_id = ?1",
    )?;
    for row in named.query_map([only], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })? {
        let (system, title, crc) = row?;
        let crcs = out.entry((system, title)).or_default();
        crcs.extend(crc.map(|c| c.to_ascii_lowercase()));
    }
    // Folder games ScummVM named.
    let mut folders = conn
        .prepare("SELECT DISTINCT system_id, title FROM folder_games WHERE ?1 IS NULL OR system_id = ?1")?;
    for row in folders.query_map([only], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        out.entry(row?).or_default();
    }
    // The base games of translations and hacks, whose metadata and art
    // they show where they have none of their own.
    let mut bases = conn.prepare(
        "SELECT DISTINCT m.system_id, m.base FROM manual_ids m
         JOIN files f ON f.sha1 = m.sha1 AND f.size = m.size AND f.status = 'manual'
         WHERE m.base IS NOT NULL AND (?1 IS NULL OR m.system_id = ?1)",
    )?;
    for row in bases.query_map([only], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        out.entry(row?).or_default();
    }
    Ok(out)
}

/// Fetch metadata and art for the games you have. Asked to stop
/// ([`EnrichOptions::cancel`]), it keeps what it saved and says so
/// (`stopped`).
pub fn enrich(
    conn: &mut Connection,
    opts: &EnrichOptions,
    mut progress: impl FnMut(EnrichEvent),
) -> Result<EnrichReport, Error> {
    let only = match &opts.system {
        Some(key) => Some(
            systems::by_key(key)
                .ok_or_else(|| Error::UnknownSystem(key.clone()))?
                .id,
        ),
        None => None,
    };
    let mut have = games_you_have(conn, only)?;
    if let Some(t) = &opts.title {
        // A translation or hack brings its base game along.
        let bases: Vec<String> = conn
            .prepare("SELECT DISTINCT base FROM manual_ids WHERE title = ?1 AND base IS NOT NULL")?
            .query_map([t], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        have.retain(|(_, game), _| game == t || bases.contains(game));
    }
    if let Some(games) = &opts.games {
        have.retain(|(system, game), _| {
            systems::by_id(*system).is_some_and(|s| games.iter().any(|(k, g)| k == s.key && g == game))
        });
    }
    let mut by_system: BTreeMap<i64, Vec<(&String, &Vec<String>)>> = BTreeMap::new();
    for ((system, game), crcs) in &have {
        by_system.entry(*system).or_default().push((game, crcs));
    }
    let mut report = EnrichReport {
        systems: by_system.len(),
        games: have.len(),
        ..EnrichReport::default()
    };

    let keys = crate::settings::load(conn)?.keys;
    scrub_art_urls(conn)?;
    // Players were once kept from libretro's DATs as `maxusers`; they are
    // `users`, as everywhere else.
    conn.execute_batch(
        "UPDATE OR IGNORE game_meta SET field = 'users' WHERE field = 'maxusers';
         DELETE FROM game_meta WHERE field = 'maxusers';",
    )?;
    // Databases the user downloaded (milestone 11), if any.
    let ovgdb = opts
        .extdb
        .as_deref()
        .and_then(|d| crate::extdb::open(crate::extdb::ExtDb::OpenVgdb, d));
    let lbdb = opts
        .extdb
        .as_deref()
        .and_then(|d| crate::extdb::open(crate::extdb::ExtDb::LaunchBox, d));
    // Sources that said "slow down" aren't asked again this run.
    let halted = std::sync::Mutex::new(Vec::new());
    // Hosts that said "slow down" (libretro's, LaunchBox's image server)
    // aren't asked again this run either, by any thread.
    let halted_hosts = std::sync::Mutex::new(std::collections::HashSet::new());
    let mut tally = Tally::new(have.len(), opts.wikidata.is_some());
    progress(EnrichEvent::Progress(crate::job::Step::new(0, 0, "starting")));
    for (system_id, games) in &by_system {
        if opts.cancel.is_set() {
            report.stopped = true;
            break;
        }
        let Some(system) = systems::by_id(*system_id) else {
            continue;
        };
        // libretro's name for the system: only its own metadata and
        // thumbnails need it; the other sources run for every system.
        let lr_name = libretro_name(system);

        let need_meta = opts.refresh || games_without_meta(conn, *system_id, games)?;
        // Metadata: one file per field; a missing file is fine.
        if let Some(lr_name) = lr_name
            && need_meta
        {
            libretro_meta(
                conn,
                opts,
                system,
                system_id,
                lr_name,
                games,
                &mut halted_hosts.lock().unwrap_or_else(|e| e.into_inner()),
                &mut report,
                &mut progress,
            )?;
        }
        // libretro's own DATs of the system (DOS, PC-98) and its title
        // lists (ScummVM): what the per-field files leave empty.
        if need_meta && !(system.dats.is_empty() && system.titles.is_empty()) {
            let found = dat_metadata(
                conn,
                opts,
                system,
                games,
                &mut halted_hosts.lock().unwrap_or_else(|e| e.into_inner()),
                &mut report,
                &mut progress,
            )?;
            let mut put = conn.prepare(
                "INSERT INTO game_meta (system_id, title, field, value) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(system_id, title, field) DO NOTHING",
            )?;
            for (game, fields) in &found {
                for (k, v) in fields {
                    put.execute(params![system_id, game, k, v])?;
                }
            }
        }
        for (game, _) in games {
            let n: i64 = conn.query_row(
                "SELECT count(*) FROM game_meta WHERE system_id = ?1 AND title = ?2",
                params![system_id, game],
                |r| r.get(0),
            )?;
            if n > 0 {
                report.games_with_metadata += 1;
            }
        }

        match system_art(
            conn,
            opts,
            system,
            system_id,
            lr_name,
            games,
            &keys,
            &lbdb,
            &halted,
            &halted_hosts,
            &mut report,
            &mut tally,
            &mut progress,
        ) {
            Err(Error::Cancelled) => {
                report.stopped = true;
                break;
            }
            r => r?,
        }
    }
    if report.stopped {
        return Ok(report);
    }
    wikidata_step(conn, opts, &have, &mut report, &mut tally, &mut progress)?;
    extdb_fill(conn, opts, &have, &ovgdb, &lbdb, &mut report)?;
    // The same failure for many games is listed once.
    let mut seen = std::collections::HashSet::new();
    report.errors.retain(|e| seen.insert(e.clone()));
    Ok(report)
}

/// Wikidata facts and pictures for the games you have.
fn wikidata_step(
    conn: &mut Connection,
    opts: &EnrichOptions,
    have: &BTreeMap<(i64, String), Vec<String>>,
    report: &mut EnrichReport,
    tally: &mut Tally,
    progress: &mut impl FnMut(EnrichEvent),
) -> Result<(), Error> {
    // Wikidata after libretro, so a release year is there to tell apart
    // games of the same name.
    if let Some(endpoint) = &opts.wikidata {
        // Translations and hacks are not on Wikidata; they show their base
        // game's facts, and the base is in `have`.
        let derived: BTreeSet<(i64, String)> = conn
            .prepare("SELECT DISTINCT system_id, title FROM manual_ids WHERE base IS NOT NULL")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        let titled: BTreeMap<(i64, String), Vec<String>> = have
            .iter()
            .filter(|(k, _)| !derived.contains(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        match crate::wikidata::enrich(conn, endpoint, &titled, opts.refresh, |step| {
            tally.wikidata(step, progress);
            progress(EnrichEvent::Wikidata { step })
        }) {
            Ok(mut w) => {
                // Logos and photos for Development, kept with the art.
                let dir = opts
                    .art_dir
                    .clone()
                    .unwrap_or_else(|| opts.cache_dir.join(crate::layout::LOOSE_ART_DIR));
                tally.wikidata("pictures", progress);
                progress(EnrichEvent::Wikidata { step: "pictures" });
                crate::wikidata::fetch_pictures(conn, &dir.join("wikidata"), &mut w.errors)?;
                report.wikidata = Some(w);
            }
            Err(e) => report.errors.push(format!("Wikidata: {e}")),
        }
    }
    Ok(())
}

/// Fields from the downloaded databases that nothing else had.
fn extdb_fill(
    conn: &Connection,
    opts: &EnrichOptions,
    have: &BTreeMap<(i64, String), Vec<String>>,
    ovgdb: &Option<Connection>,
    lbdb: &Option<Connection>,
    report: &mut EnrichReport,
) -> Result<(), Error> {
    // Last, the downloaded databases fill fields nothing else had:
    // OpenVGDB by hash first, then LaunchBox by title.
    if ovgdb.is_some() || lbdb.is_some() {
        for (system_id, game) in have.keys() {
            let Some(system) = systems::by_id(*system_id) else {
                continue;
            };
            let mut found: Vec<(&'static str, &'static str, String)> = Vec::new();
            if let Some(db) = &ovgdb {
                match crate::extdb::openvgdb_lookup(db, &game_roms(conn, *system_id, game)?) {
                    Ok(Some(f)) => found.extend(f.fields.into_iter().map(|(k, v)| ("openvgdb", k, v))),
                    Ok(None) => {}
                    Err(e) => report.errors.push(format!("OpenVGDB: {e}")),
                }
            }
            if let Some(db) = &lbdb {
                match crate::extdb::launchbox_lookup(db, system, game, &opts.launchbox_images) {
                    Ok(Some(f)) => found.extend(f.fields.into_iter().map(|(k, v)| ("launchbox", k, v))),
                    Ok(None) => {}
                    Err(e) => report.errors.push(format!("LaunchBox: {e}")),
                }
            }
            let mut filled = false;
            for (source, field, value) in found {
                // Only empty fields, or libretro's "???" placeholders.
                let n = conn.execute(
                    "INSERT INTO game_meta (system_id, title, field, value, source) VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(system_id, title, field) DO UPDATE SET value = excluded.value, source = excluded.source
                     WHERE game_meta.value LIKE '%???%' OR game_meta.source = excluded.source",
                    params![system_id, game, field, value, source],
                )?;
                filled |= n > 0;
            }
            if filled {
                report.extdb_games += 1;
            }
        }
    }
    Ok(())
}

/// The SHA1s of a game's ROMs (from its DATs) or of the files you named it.
fn game_roms(conn: &Connection, system_id: i64, title: &str) -> Result<Vec<(String, u64)>, Error> {
    Ok(conn
        .prepare_cached(
            "SELECT r.sha1, r.size FROM games g JOIN dats d ON d.dat_id = g.dat_id
               JOIN roms r ON r.game_id = g.game_id
             WHERE d.system_id = ?1 AND g.name = ?2 AND r.sha1 IS NOT NULL AND r.size IS NOT NULL
             UNION SELECT sha1, size FROM manual_ids WHERE system_id = ?1 AND title = ?2",
        )?
        .query_map(params![system_id, title], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?)
}
