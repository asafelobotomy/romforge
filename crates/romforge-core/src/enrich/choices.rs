use super::*;

/// A picture the art picker offers for a game.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ArtChoice {
    /// The full picture, downloaded when picked.
    pub url: String,
    /// "libretro", "launchbox", "screenscraper", "steamgriddb" or "igdb".
    pub source: String,
    /// Its region, size or style, where the source says.
    pub label: Option<String>,
    /// A small copy, downloaded to show (a file path).
    pub preview: String,
    /// The game's picture now.
    pub current: bool,
}

/// What [`art_choices`] found, and what went wrong on the way.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ArtChoices {
    pub choices: Vec<ArtChoice>,
    pub errors: Vec<String>,
}

/// Previews kept for the picker.
const CHOICES_DIR: &str = "art-choices";

/// Every picture of `kind` the sources have for a game (`lookup`: the title
/// to look it up by; `game`: the title it is kept under): libretro's, the
/// LaunchBox database's (if downloaded), and each keyed source's, best
/// first, at most `limit`, with a small preview of each downloaded into the
/// cache. Asks the network every time (nothing here is kept but previews).
pub fn art_choices(
    conn: &Connection,
    opts: &EnrichOptions,
    system_id: i64,
    game: &str,
    lookup: &str,
    kind: ArtKind,
    limit: usize,
) -> Result<ArtChoices, Error> {
    let system = systems::by_id(system_id).ok_or_else(|| Error::UnknownSystem(system_id.to_string()))?;
    let mut out = ArtChoices::default();
    let mut found: Vec<(String, ArtFound)> = Vec::new();
    if let Some(lr) = libretro_name(system) {
        let lr_game = libretro_game(conn, system, system_id, lookup)?;
        if let Some(url) = thumbnail_url(&opts.thumbnails_base, lr, kind, &lr_game) {
            found.push(("libretro".into(), ArtFound::new(kind, url)));
        }
    }
    if let Some(lb) = opts
        .extdb
        .as_deref()
        .and_then(|d| crate::extdb::open(crate::extdb::ExtDb::LaunchBox, d))
    {
        match crate::extdb::launchbox_lookup(&lb, system, lookup, &opts.launchbox_images) {
            Ok(Some(f)) => found.extend(
                f.images
                    .into_iter()
                    .filter(|i| i.kind == kind)
                    .map(|i| ("launchbox".to_owned(), i)),
            ),
            Ok(None) => {}
            Err(e) => out.errors.push(format!("LaunchBox: {e}")),
        }
    }
    let settings = crate::settings::load(conn)?;
    let keys = &settings.keys;
    let prefs = settings.region_order();
    let mut session = crate::artsources::Session::default();
    for provider in crate::artsources::Provider::available(keys, system) {
        let job = crate::artsources::Job {
            system,
            title: lookup,
            rom: crate::artsources::rom_for(conn, system_id, lookup)?,
            prefs: &prefs,
            language: settings.language(),
            year: None,
            serial: crate::artsources::serial_for(
                conn,
                system_id,
                lookup,
                &crate::artsources::gametdb_kinds().collect::<Vec<_>>(),
            )?,
            video: false,
        };
        match crate::artsources::lookup(provider, keys, &opts.sources, &mut session, &job) {
            Ok(f) => found.extend(
                f.art
                    .into_iter()
                    .filter(|i| i.kind == kind)
                    .map(|i| (provider.name().to_owned(), i)),
            ),
            Err(e) => out.errors.push(format!("{}: {e}", provider.name())),
        }
    }
    let current: Option<String> = conn
        .query_row(
            "SELECT url FROM art WHERE system_id = ?1 AND title = ?2 AND kind = ?3",
            params![system_id, game, kind.as_str()],
            |r| r.get(0),
        )
        .optional()?
        .flatten()
        .map(|u: String| crate::artsources::without_credentials(&u));
    let dir = opts.cache_dir.join(CHOICES_DIR);
    let mut halted = std::collections::HashSet::new();
    let mut seen = std::collections::HashSet::new();
    for (source, mut f) in found {
        if out.choices.len() >= limit {
            break;
        }
        // Credentials never reach the app; they are put back to download.
        f.url = crate::artsources::without_credentials(&f.url);
        if !seen.insert(f.url.clone()) {
            continue;
        }
        let show = f.thumb.as_deref().unwrap_or(&f.url);
        let stem = crate::hash::hex(&{
            use sha1::Digest as _;
            sha1::Sha1::digest(show.as_bytes())
        });
        // Kept from an earlier look, else downloaded now.
        let cached = ["png", "jpg", "webp"]
            .iter()
            .map(|e| dir.join(format!("{stem}.{e}")))
            .find(|p| p.exists());
        let preview = match cached {
            Some(p) => p,
            None => match get_image_halting(
                &crate::artsources::download_url(&source, show, keys, &opts.sources),
                &mut halted,
            ) {
                Ok(Some(data)) => match image_ext(&data) {
                    Some(ext) => {
                        let p = dir.join(format!("{stem}.{ext}"));
                        save(&p, &data)?;
                        p
                    }
                    None => continue,
                },
                Ok(None) => continue,
                Err(e) => {
                    out.errors.push(format!("{source}: {e}"));
                    continue;
                }
            },
        };
        out.choices.push(ArtChoice {
            current: current.as_deref() == Some(f.url.as_str()),
            url: f.url,
            source,
            label: f.label,
            preview: preview.to_string_lossy().into_owned(),
        });
    }
    Ok(out)
}

/// Make the picture at `url` (from [`art_choices`]) the game's `kind`: it
/// is downloaded in full, kept as Enrich keeps art, and marked as picked so
/// Enrich never replaces it.
#[allow(clippy::too_many_arguments)]
pub fn choose_art(
    conn: &Connection,
    opts: &EnrichOptions,
    system_id: i64,
    game: &str,
    kind: ArtKind,
    url: &str,
    source: &str,
) -> Result<(), Error> {
    let system = systems::by_id(system_id).ok_or_else(|| Error::UnknownSystem(system_id.to_string()))?;
    let keys = crate::settings::load(conn)?.keys;
    let get = crate::artsources::download_url(source, url, &keys, &opts.sources);
    let data =
        fetch::get_image(&get)?.ok_or_else(|| Error::Http(format!("{}: HTTP 404", fetch::short(url))))?;
    if image_ext(&data).is_none() {
        return Err(Error::Http(format!(
            "{}: not an image romforge shows",
            fetch::short(url)
        )));
    }
    let old: Option<String> = conn
        .query_row(
            "SELECT path FROM art WHERE system_id = ?1 AND title = ?2 AND kind = ?3",
            params![system_id, game, kind.as_str()],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    keep(conn, opts, system, system_id, game, kind, &data, source, url)?;
    // A PNG replaced by a JPEG: the old file goes.
    let new: Option<String> = conn.query_row(
        "SELECT path FROM art WHERE system_id = ?1 AND title = ?2 AND kind = ?3",
        params![system_id, game, kind.as_str()],
        |r| r.get(0),
    )?;
    if let Some(old) = old
        && Some(&old) != new.as_ref()
    {
        let _ = std::fs::remove_file(old);
    }
    conn.execute(
        "UPDATE art SET chosen = 1 WHERE system_id = ?1 AND title = ?2 AND kind = ?3",
        params![system_id, game, kind.as_str()],
    )?;
    Ok(())
}
