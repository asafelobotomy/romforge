use super::*;

/// libretro's per-field metadata files of one system, for its games.
#[allow(clippy::too_many_arguments)]
pub(super) fn libretro_meta(
    conn: &mut Connection,
    opts: &EnrichOptions,
    system: &'static systems::System,
    system_id: &i64,
    lr_name: &str,
    games: &[(&String, &Vec<String>)],
    halted_hosts: &mut std::collections::HashSet<String>,
    report: &mut EnrichReport,
    progress: &mut impl FnMut(EnrichEvent),
) -> Result<(), Error> {
    let mut found: HashMap<&String, Vec<(String, String)>> = HashMap::new();
    let mut fetch_failed = false;
    for field in FIELDS {
        progress(EnrichEvent::Metadata {
            system: system.key,
            field,
        });
        let url = format!(
            "{}/{field}/{}.dat",
            opts.metadata_base,
            fetch::encode_segment(lr_name)
        );
        let body = match get_halting(&url, halted_hosts) {
            Ok(Some(b)) => b,
            Ok(None) => continue,
            Err(e) => {
                report.errors.push(e.to_string());
                fetch_failed = true;
                continue;
            }
        };
        let records = match dat::parse_cmp_records(&String::from_utf8_lossy(&body)) {
            Ok(r) => r,
            Err(e) => {
                report.errors.push(format!("{url}: {e}"));
                fetch_failed = true;
                continue;
            }
        };
        let by_title: HashMap<&str, &dat::Record> = records
            .iter()
            .filter_map(|r| Some((r.title.as_deref()?, r)))
            .collect();
        let mut by_crc: HashMap<&str, &dat::Record> = HashMap::new();
        for r in &records {
            for c in &r.crcs {
                by_crc.entry(c.as_str()).or_insert(r);
            }
        }
        for (game, crcs) in games {
            let rec = by_title
                .get(game.as_str())
                .or_else(|| crcs.iter().find_map(|c| by_crc.get(c.as_str())));
            if let Some(rec) = rec {
                let f = found.entry(game).or_default();
                f.extend(
                    rec.fields
                        .iter()
                        .map(|(k, v)| (libretro_field(k).to_owned(), v.clone())),
                );
                // Found by CRC under another name (No-Intro renamed
                // it since): libretro's thumbnails go by that name.
                if let Some(t) = rec.title.as_deref().filter(|t| *t != game.as_str()) {
                    f.push((LIBRETRO_NAME.to_owned(), t.to_owned()));
                }
            }
        }
    }
    let tx = conn.transaction()?;
    {
        for (game, fields) in &found {
            if fetch_failed {
                // A file missing this time is no reason to drop a date.
                for (k, v) in fields {
                    put_field(&tx, *system_id, game, k, v, LIBRETRO, false)?;
                }
                continue;
            }
            // Year and month as one date ([`put_fields`]).
            let fields: Vec<(&str, String)> = fields.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
            put_fields(&tx, *system_id, game, &fields, LIBRETRO)?;
        }
        let mut checked = tx.prepare(concat!(
            "INSERT INTO meta_checked (system_id, title, checked_at)
             VALUES (?1, ?2, ",
            crate::db::now!(),
            ")
             ON CONFLICT(system_id, title) DO UPDATE SET checked_at = excluded.checked_at"
        ))?;
        // After a failed download, look again next time.
        if !fetch_failed {
            for (game, _) in games {
                checked.execute(params![system_id, game])?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

/// Fields found per game: (field, value).
type FoundMeta<'g> = HashMap<&'g String, Vec<(String, String)>>;

/// Metadata from libretro's own DATs and title lists of `system`
/// (`dat/<name>.dat`): a game by its title, its CRC32s, or (a ScummVM
/// folder) its game id. Their genre, developer, publisher, franchise,
/// year and players; a title list's description too (a DAT's is a
/// variant name). When libretro names the game otherwise, that name, for
/// its thumbnails.
pub(super) fn dat_metadata<'g>(
    conn: &Connection,
    opts: &EnrichOptions,
    system: &'static systems::System,
    games: &[(&'g String, &Vec<String>)],
    halted_hosts: &mut std::collections::HashSet<String>,
    report: &mut EnrichReport,
    progress: &mut impl FnMut(EnrichEvent),
) -> Result<FoundMeta<'g>, Error> {
    let mut found: FoundMeta<'g> = HashMap::new();
    let mut code_of = conn.prepare_cached(
        "SELECT code FROM folder_games WHERE system_id = ?1 AND title = ?2 ORDER BY folder_id",
    )?;
    let sources = system
        .dats
        .iter()
        .map(|n| (*n, false))
        .chain(system.titles.iter().map(|n| (*n, true)));
    for (name, overview) in sources {
        progress(EnrichEvent::Metadata {
            system: system.key,
            field: name,
        });
        let url = format!("{}/{}.dat", opts.dats_base, fetch::encode_segment(name));
        let body = match get_halting(&url, halted_hosts) {
            Ok(Some(b)) => b,
            Ok(None) => continue,
            Err(e) => {
                report.errors.push(e.to_string());
                continue;
            }
        };
        let records = match dat::parse_cmp_records(&String::from_utf8_lossy(&body)) {
            Ok(r) => r,
            Err(e) => {
                report.errors.push(format!("{url}: {e}"));
                continue;
            }
        };
        let mut by_title: HashMap<&str, &dat::Record> = HashMap::new();
        let mut by_crc: HashMap<&str, &dat::Record> = HashMap::new();
        let mut by_code: HashMap<&str, &dat::Record> = HashMap::new();
        for r in &records {
            if let Some(t) = r.title.as_deref() {
                by_title.entry(t).or_insert(r);
            }
            for c in &r.crcs {
                by_crc.entry(c.as_str()).or_insert(r);
            }
            if let Some((_, c)) = r.fields.iter().find(|(k, _)| k == "code") {
                by_code.entry(c.as_str()).or_insert(r);
            }
        }
        for (game, crcs) in games {
            let code: Option<String> = code_of
                .query_row(params![system.id, game], |r| r.get(0))
                .optional()?;
            let rec = by_title
                .get(game.as_str())
                .or_else(|| crcs.iter().find_map(|c| by_crc.get(c.as_str())))
                .or_else(|| code.as_deref().and_then(|c| by_code.get(c)));
            let Some(rec) = rec else { continue };
            let f = found.entry(*game).or_default();
            for (k, v) in &rec.fields {
                // Players come as `users`, as in libretro's `maxusers` files,
                // and are kept so (the field the app and Cleanup read).
                let file = if k == "users" { "maxusers" } else { k.as_str() };
                if FIELDS.contains(&file) || (overview && k == "description") {
                    f.push((k.clone(), v.clone()));
                }
            }
            if let Some(t) = rec.title.as_deref().filter(|t| *t != game.as_str()) {
                f.push((LIBRETRO_NAME.to_owned(), t.to_owned()));
            }
        }
    }
    Ok(found)
}

/// Whether some game's metadata was never looked up (it may be newly
/// scanned).
pub(super) fn games_without_meta(
    conn: &Connection,
    system_id: i64,
    games: &[(&String, &Vec<String>)],
) -> Result<bool, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT EXISTS (SELECT 1 FROM meta_checked WHERE system_id = ?1 AND title = ?2 AND checked_at >= ?3)",
    )?;
    for (game, _) in games {
        if !stmt.query_row(params![system_id, game, ASK_AGAIN_SINCE], |r| r.get::<_, bool>(0))? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The name libretro files a game's thumbnails under: its own name for
/// it, an arcade set's DAT description ("Street Fighter II - The World
/// Warrior ..."), else the DAT name.
pub(crate) fn libretro_game(
    conn: &Connection,
    system: &systems::System,
    system_id: i64,
    game: &str,
) -> Result<String, Error> {
    let own: Option<String> = conn
        .query_row(
            "SELECT value FROM game_meta WHERE system_id = ?1 AND title = ?2 AND field = ?3",
            params![system_id, game, LIBRETRO_NAME],
            |r| r.get(0),
        )
        .optional()?;
    let described = match own {
        Some(n) => Some(n),
        None if system.arcade => conn
            .query_row(
                "SELECT g.description FROM games g JOIN dats d ON d.dat_id = g.dat_id
                 WHERE d.system_id = ?1 AND g.name = ?2 AND g.description IS NOT NULL
                 ORDER BY g.game_id LIMIT 1",
                params![system_id, game],
                |r| r.get::<_, String>(0),
            )
            .optional()?,
        None => None,
    };
    Ok(described.unwrap_or_else(|| game.to_owned()))
}
