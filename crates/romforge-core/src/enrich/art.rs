use super::*;

mod ask;
mod game;

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
/// keyed sources. Games are fetched on up to four of the Settings
/// `threads` at once (each host still gets no more than its own pace,
/// [`fetch`]); each game's sources keep their order, and what was found is
/// recorded here in game order. `done` counts games for progress.
#[allow(clippy::too_many_arguments)]
pub(super) fn system_art(
    conn: &Connection,
    opts: &EnrichOptions,
    system: &'static systems::System,
    system_id: &i64,
    lr_name: Option<&str>,
    games: &[(&String, &Vec<String>)],
    settings: &crate::settings::Settings,
    lbdb: &Option<Connection>,
    halted: &std::sync::Mutex<Vec<crate::artsources::Provider>>,
    halted_hosts: &std::sync::Mutex<std::collections::HashSet<String>>,
    report: &mut EnrichReport,
    tally: &mut Tally,
    progress: &mut impl FnMut(EnrichEvent),
) -> Result<(), Error> {
    let prefs = settings.region_order();
    let mut inputs = Vec::with_capacity(games.len());
    for (game, _) in games {
        let input = game::prepare(
            conn,
            opts,
            system,
            *system_id,
            lr_name,
            game,
            &settings.keys,
            &prefs,
            settings.video_snaps,
            lbdb,
        )?;
        report.art_cached += input.cached;
        inputs.push(input);
    }
    let shared = game::Shared {
        opts,
        system,
        keys: &settings.keys,
        prefs: &prefs,
        language: settings.language(),
        video: settings.video_snaps,
        halted_hosts,
        halted,
    };
    let threads = crate::parallel::threads(opts.threads).min(4);
    let mut failed = Ok(());
    let mut each = |out: Option<game::GameOut>| {
        let Some(out) = out else { return };
        if failed.is_ok() {
            failed = game::record_out(conn, *system_id, &out, report);
        }
        tally.game(system.key, &out.game, progress);
    };
    let fetch = |input| (!opts.cancel.is_set()).then(|| game::fetch(&shared, input));
    // The first game alone: a host that says "slow down" says it before
    // the others ask it too.
    let rest = inputs.split_off(inputs.len().min(1));
    for input in inputs {
        each(fetch(input));
    }
    crate::parallel::map_ordered(rest, threads, fetch, |_, out| each(out));
    failed?;
    opts.cancel.check()
}

/// [`fetch::get_optional`], unless `url`'s host rate-limited this run; a
/// host that does now is added to `halted`.
pub(super) fn get_halting(
    url: &str,
    halted: &mut std::collections::HashSet<String>,
) -> Result<Option<Vec<u8>>, Error> {
    halting(&fetch::Request::get(url), halted)
}

/// [`get_halting`] for a picture ([`fetch::Request::image`]'s limits).
pub(super) fn get_image_halting(
    url: &str,
    halted: &mut std::collections::HashSet<String>,
) -> Result<Option<Vec<u8>>, Error> {
    halting(&fetch::Request::image(url), halted)
}

/// The hosts that rate-limited this run: one worker's own, or shared.
pub(super) trait Halted {
    fn has(&self, host: &str) -> bool;
    fn add(&mut self, host: String);
}

impl Halted for std::collections::HashSet<String> {
    fn has(&self, host: &str) -> bool {
        self.contains(host)
    }
    fn add(&mut self, host: String) {
        self.insert(host);
    }
}

impl Halted for &std::sync::Mutex<std::collections::HashSet<String>> {
    fn has(&self, host: &str) -> bool {
        self.lock().unwrap_or_else(|e| e.into_inner()).contains(host)
    }
    fn add(&mut self, host: String) {
        self.lock().unwrap_or_else(|e| e.into_inner()).insert(host);
    }
}

/// [`fetch::request`], unless `req`'s host rate-limited this run; a host
/// that does now is added to `halted`.
pub(super) fn halting(req: &fetch::Request, halted: &mut impl Halted) -> Result<Option<Vec<u8>>, Error> {
    let host = fetch::host_of(req.url).to_owned();
    if halted.has(&host) {
        return Err(Error::RateLimited(format!(
            "{host}: rate limiting, not asked again this run"
        )));
    }
    let r = fetch::request(req);
    if let Err(e) = &r
        && fetch::is_rate_limited(e)
    {
        halted.add(host);
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
    let path = game::art_path(opts, system, game, kind, image_ext(data).unwrap_or("png"));
    save(&path, data).map_err(|e| Error::Http(format!("{}: {e}", path.display())))?;
    record(conn, system_id, game, kind, Some(&path.to_string_lossy()), source)?;
    conn.execute(
        "UPDATE art SET url = ?4, chosen = 0 WHERE system_id = ?1 AND title = ?2 AND kind = ?3",
        params![
            system_id,
            game,
            kind.as_str(),
            crate::artsources::without_credentials(url)
        ],
    )?;
    Ok(())
}

/// Note what was found for a game (`path` None: looked for, not found).
pub(super) fn record(
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

/// Write `data` to `path` via a temporary file of its own, so a crash never
/// leaves half an image under the real name.
pub(super) fn save(path: &Path, data: &[u8]) -> std::io::Result<()> {
    crate::atomic::write(path, data)
}

/// Art URLs kept before credentials were left out of them (ScreenScraper's
/// media URLs carry the owner's): kept without them now.
pub(super) fn scrub_art_urls(conn: &Connection) -> Result<(), Error> {
    let rows: Vec<(i64, String, String, String)> = conn
        .prepare(
            "SELECT system_id, title, kind, url FROM art
             WHERE url LIKE '%devpassword=%' OR url LIKE '%sspassword=%'
                OR url LIKE '%devid=%' OR url LIKE '%ssid=%'",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    for (system_id, title, kind, url) in rows {
        conn.execute(
            "UPDATE art SET url = ?4 WHERE system_id = ?1 AND title = ?2 AND kind = ?3",
            params![
                system_id,
                title,
                kind,
                crate::artsources::without_credentials(&url)
            ],
        )?;
    }
    Ok(())
}
