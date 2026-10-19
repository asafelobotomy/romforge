use super::*;

/// libretro's thumbnail file name: `&*/:`<>?\|"` become `_`.
pub fn thumbnail_file(game: &str) -> String {
    let name: String = game
        .chars()
        .map(|c| if "&*/:`<>?\\|\"".contains(c) { '_' } else { c })
        .collect();
    format!("{name}.png")
}

/// A game's file name in the art cache, without the extension: libretro's
/// name when that is the title itself; otherwise (characters replaced, so
/// "A: B" and "A? B" would share one file, or too long for a file name)
/// cut to 200 bytes and marked with the start of the title's CRC32.
pub(crate) fn cache_stem(game: &str) -> String {
    let file = thumbnail_file(game);
    let stem = file.strip_suffix(".png").unwrap_or(&file);
    if stem == game && stem.len() <= 200 {
        return stem.to_owned();
    }
    let mut cut = stem.len().min(200);
    while !stem.is_char_boundary(cut) {
        cut -= 1;
    }
    format!(
        "{} [{:08x}]",
        stem[..cut].trim_end(),
        crc32fast::hash(game.as_bytes())
    )
}

pub fn thumbnail_url(base: &str, system_name: &str, kind: ArtKind, game: &str) -> Option<String> {
    Some(format!(
        "{base}/{}/{}/{}",
        fetch::encode_segment(system_name),
        kind.folder()?,
        fetch::encode_segment(&thumbnail_file(game))
    ))
}

/// The file type of an image by its first bytes.
pub fn image_ext(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG") {
        Some("png")
    } else if data.starts_with(b"\xff\xd8\xff") {
        Some("jpg")
    } else if data.len() > 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

/// Art for the games of one system: libretro, then LaunchBox, then the
/// keyed sources.
#[allow(clippy::too_many_arguments)]
pub(super) fn system_art(
    conn: &Connection,
    opts: &EnrichOptions,
    system: &'static systems::System,
    system_id: &i64,
    lr_name: Option<&str>,
    games: &[(&String, &Vec<String>)],
    keys: &crate::settings::Keys,
    lbdb: &Option<Connection>,
    session: &mut crate::artsources::Session,
    halted: &mut Vec<crate::artsources::Provider>,
    halted_hosts: &mut std::collections::HashSet<String>,
    report: &mut EnrichReport,
    progress: &mut impl FnMut(EnrichEvent),
) -> Result<(), Error> {
    // Art: libretro first (house rule 10, tier 1); the keyed sources
    // fill what it doesn't have.
    for (game, _) in games {
        let lr_game = libretro_game(conn, system, *system_id, game)?;
        let lr_game = lr_game.as_str();
        let mut missing: Vec<ArtKind> = Vec::new();
        for &kind in &opts.art {
            let (known, chosen): (Option<Option<String>>, bool) = conn
                .query_row(
                    "SELECT path, chosen FROM art WHERE system_id = ?1 AND title = ?2 AND kind = ?3",
                    params![system_id, game, kind.as_str()],
                    |r| Ok((Some(r.get(0)?), r.get(1)?)),
                )
                .optional()?
                .unwrap_or((None, false));
            let still_there = matches!(&known, Some(Some(p)) if Path::new(p).exists());
            // One the owner picked stays, even when refreshing.
            if still_there && (!opts.refresh || chosen) {
                report.art_cached += 1;
                continue;
            }
            // libretro has no such kind, or didn't have this one before.
            let Some(url) = lr_name
                .and_then(|n| thumbnail_url(&opts.thumbnails_base, n, kind, lr_game))
                .filter(|_| opts.refresh || !matches!(known, Some(None)))
            else {
                missing.push(kind);
                continue;
            };
            progress(EnrichEvent::Art {
                system: system.key,
                game: (*game).clone(),
                kind,
            });
            match get_halting(&url, halted_hosts) {
                Ok(Some(data)) if image_ext(&data).is_some() => {
                    match keep(
                        conn, opts, system, *system_id, game, kind, &data, "libretro", &url,
                    ) {
                        Ok(()) => report.art_saved += 1,
                        Err(e) => report.errors.push(e.to_string()),
                    }
                }
                Ok(_) => {
                    record(conn, *system_id, game, kind, None, "libretro")?;
                    missing.push(kind);
                }
                Err(e) => report.errors.push(e.to_string()),
            }
        }
        // LaunchBox next (free, no key; downloaded by the user): only
        // what is still missing.
        if !missing.is_empty()
            && let Some(lb) = &lbdb
        {
            match crate::extdb::launchbox_lookup(lb, system, game, &opts.launchbox_images) {
                Ok(Some(found)) => {
                    for ArtFound { kind, url, .. } in found.images {
                        if !missing.contains(&kind) {
                            continue;
                        }
                        progress(EnrichEvent::Art {
                            system: system.key,
                            game: (*game).clone(),
                            kind,
                        });
                        match get_halting(&url, halted_hosts) {
                            Ok(Some(data)) if image_ext(&data).is_some() => {
                                match keep(
                                    conn,
                                    opts,
                                    system,
                                    *system_id,
                                    game,
                                    kind,
                                    &data,
                                    "launchbox",
                                    &url,
                                ) {
                                    Ok(()) => {
                                        report.art_saved += 1;
                                        missing.retain(|k| *k != kind);
                                    }
                                    Err(e) => report.errors.push(e.to_string()),
                                }
                            }
                            Ok(_) => {}
                            Err(e) => report.errors.push(format!("LaunchBox: {e}")),
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => report.errors.push(format!("LaunchBox: {e}")),
            }
        }
        // Keyed sources, each asked once per game unless refreshing.
        let providers: Vec<_> = crate::artsources::Provider::available(keys, system)
            .into_iter()
            .filter(|p| !halted.contains(p))
            .collect();
        for provider in providers {
            if missing.is_empty() {
                break;
            }
            let asked: bool = conn
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM art_lookup WHERE system_id = ?1 AND title = ?2 AND provider = ?3)",
                    params![system_id, game, provider.name()],
                    |r| r.get(0),
                )?;
            if asked && !opts.refresh {
                continue;
            }
            let job = crate::artsources::Job {
                system,
                title: game,
                rom: crate::artsources::rom_for(conn, *system_id, game)?,
            };
            match crate::artsources::lookup(provider, keys, &opts.sources, session, &job) {
                Ok(found) => {
                    for ArtFound { kind, url, .. } in found {
                        if !missing.contains(&kind) {
                            continue;
                        }
                        progress(EnrichEvent::Art {
                            system: system.key,
                            game: (*game).clone(),
                            kind,
                        });
                        match get_halting(&url, halted_hosts) {
                            Ok(Some(data)) if image_ext(&data).is_some() => {
                                match keep(
                                    conn,
                                    opts,
                                    system,
                                    *system_id,
                                    game,
                                    kind,
                                    &data,
                                    provider.name(),
                                    &url,
                                ) {
                                    Ok(()) => {
                                        report.art_saved += 1;
                                        missing.retain(|k| *k != kind);
                                    }
                                    Err(e) => report.errors.push(e.to_string()),
                                }
                            }
                            Ok(_) => {}
                            Err(e) => report.errors.push(e.to_string()),
                        }
                    }
                    conn.execute(
                        concat!("INSERT INTO art_lookup (system_id, title, provider, checked_at)
                         VALUES (?1, ?2, ?3, ", crate::db::now!(), ")
                         ON CONFLICT(system_id, title, provider) DO UPDATE SET checked_at = excluded.checked_at"),
                        params![system_id, game, provider.name()],
                    )?;
                }
                Err(e) => {
                    if fetch::is_rate_limited(&e) {
                        halted.push(provider);
                    }
                    report.errors.push(format!("{}: {e}", provider.name()));
                }
            }
        }
        report.art_missing += missing.len();
    }
    Ok(())
}

/// [`fetch::get_optional`], unless `url`'s host rate-limited this run; a
/// host that does now is added to `halted`.
pub(super) fn get_halting(
    url: &str,
    halted: &mut std::collections::HashSet<String>,
) -> Result<Option<Vec<u8>>, Error> {
    let host = url
        .split("://")
        .nth(1)
        .and_then(|r| r.split('/').next())
        .unwrap_or("")
        .to_owned();
    if halted.contains(&host) {
        return Err(Error::Http(format!(
            "{host}: rate limiting, not asked again this run"
        )));
    }
    let r = fetch::get_optional(url);
    if let Err(e) = &r
        && fetch::is_rate_limited(e)
    {
        halted.insert(host);
    }
    r
}

/// Save an image for a game and record it.
#[allow(clippy::too_many_arguments)]
pub(super) fn keep(
    conn: &Connection,
    opts: &EnrichOptions,
    system: &systems::System,
    system_id: i64,
    game: &str,
    kind: ArtKind,
    data: &[u8],
    source: &str,
    url: &str,
) -> Result<(), Error> {
    let ext = image_ext(data).unwrap_or("png");
    let path = opts
        .art_dir
        .clone()
        .unwrap_or_else(|| opts.cache_dir.join(crate::layout::LOOSE_ART_DIR))
        .join(system.key)
        .join(kind.as_str())
        .join(format!("{}.{ext}", cache_stem(game)));
    save(&path, data).map_err(|e| Error::Http(format!("{}: {e}", path.display())))?;
    record(conn, system_id, game, kind, Some(&path.to_string_lossy()), source)?;
    conn.execute(
        "UPDATE art SET url = ?4, chosen = 0 WHERE system_id = ?1 AND title = ?2 AND kind = ?3",
        params![system_id, game, kind.as_str(), url],
    )?;
    Ok(())
}

/// Note what was found for a game (`path` None: looked for, not found).
fn record(
    conn: &Connection,
    system_id: i64,
    game: &str,
    kind: ArtKind,
    path: Option<&str>,
    source: &str,
) -> Result<(), Error> {
    conn.execute(
        concat!(
            "INSERT INTO art (system_id, title, kind, path, checked_at, source)
         VALUES (?1, ?2, ?3, ?4, ",
            crate::db::now!(),
            ", ?5)
         ON CONFLICT(system_id, title, kind) DO UPDATE SET path = excluded.path,
           checked_at = excluded.checked_at, source = excluded.source
         -- A source that found nothing never wipes a picture another source
         -- saved (a refresh asks libretro again before the others).
         WHERE excluded.path IS NOT NULL OR art.path IS NULL
            OR coalesce(art.source, 'libretro') = excluded.source"
        ),
        params![system_id, game, kind.as_str(), path, source],
    )?;
    Ok(())
}

/// Write `data` to `path` via a temporary file, so a crash never leaves half
/// an image under the real name.
pub(super) fn save(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}
