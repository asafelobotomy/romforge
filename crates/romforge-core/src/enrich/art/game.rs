//! One game's art: what the database says first (on the calling thread),
//! the downloads on a worker (libretro, then LaunchBox, then the keyed
//! sources, in that order: house rule 10), then what was found recorded
//! (on the calling thread again).

use std::collections::HashSet;
use std::sync::Mutex;

use super::super::*;
use super::{image_ext, thumbnail_url};
use crate::artsources::{Provider, RomRef};

/// What a worker needs for one game, read beforehand.
pub(super) struct GameIn {
    pub game: String,
    /// libretro's thumbnails to try, by kind: the game's own, then its
    /// other versions' (best region first).
    libretro: Vec<(ArtKind, Vec<String>)>,
    /// Kinds libretro can't give (or didn't before): for the others.
    missing: Vec<ArtKind>,
    /// LaunchBox's pictures of the game (from the downloaded database).
    launchbox: Vec<ArtFound>,
    /// Keyed sources to ask (not asked before, unless refreshing).
    providers: Vec<Provider>,
    rom: Option<RomRef>,
    /// Its release year, if known (IGDB tells same-named games apart by it).
    year: Option<i32>,
    /// Its header's serial (GameTDB finds games by it).
    serial: Option<crate::ident::GameId>,
    /// Errors found while reading (LaunchBox's database).
    errors: Vec<String>,
    pub cached: usize,
}

/// What a worker found for one game.
pub(super) enum Got {
    /// Saved at `path`, from `source` (`url` without credentials later).
    Saved {
        kind: ArtKind,
        path: String,
        source: String,
        url: String,
    },
    /// libretro has no such picture.
    NotOnLibretro(ArtKind),
    /// A keyed source was asked (remembered so it isn't asked again).
    Asked(Provider),
    /// Fields of text a keyed source has for the game.
    Fields(Provider, Vec<(&'static str, String)>),
    Error(String),
}

pub(super) struct GameOut {
    pub game: String,
    pub got: Vec<Got>,
    pub missing: usize,
}

/// Everything shared by one system's games on the workers.
pub(super) struct Shared<'a> {
    pub opts: &'a EnrichOptions,
    pub system: &'static systems::System,
    pub keys: &'a crate::settings::Keys,
    /// Regions, best first (Settings).
    pub prefs: &'a [String],
    /// The language for text (Settings).
    pub language: &'static str,
    /// Video snaps are wanted (Settings).
    pub video: bool,
    /// Hosts and keyed sources that said "slow down": not asked again
    /// this run, by any worker.
    pub halted_hosts: &'a Mutex<HashSet<String>>,
    pub halted: &'a Mutex<Vec<Provider>>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    conn: &Connection,
    opts: &EnrichOptions,
    system: &'static systems::System,
    system_id: i64,
    lr_name: Option<&str>,
    game: &str,
    keys: &crate::settings::Keys,
    prefs: &[String],
    video: bool,
    lbdb: &Option<Connection>,
) -> Result<GameIn, Error> {
    let lr_game = libretro_game(conn, system, system_id, game)?;
    // Another version's picture when this one's is missing, best region
    // first (a few: each is a request).
    let others = super::super::regions::other_versions(conn, system_id, game, prefs, 3)?;
    let mut input = GameIn {
        game: game.to_owned(),
        libretro: Vec::new(),
        missing: Vec::new(),
        launchbox: Vec::new(),
        providers: Vec::new(),
        rom: None,
        year: None,
        serial: None,
        errors: Vec::new(),
        cached: 0,
    };
    for &kind in opts.art.iter().filter(|k| video || **k != ArtKind::Video) {
        // A miss from before the sources learnt more counts as not asked.
        let (known, chosen): (Option<Option<String>>, bool) = conn
            .query_row(
                "SELECT path, chosen, checked_at < ?4 FROM art WHERE system_id = ?1 AND title = ?2 AND kind = ?3",
                params![system_id, game, kind.as_str(), ASK_AGAIN_SINCE],
                |r| {
                    let path: Option<String> = r.get(0)?;
                    let old: bool = r.get(2)?;
                    Ok(((path.is_some() || !old).then_some(path), r.get(1)?))
                },
            )
            .optional()?
            .unwrap_or((None, false));
        let still_there = matches!(&known, Some(Some(p)) if Path::new(p).exists());
        // One the owner picked stays, even when refreshing.
        if still_there && (!opts.refresh || chosen) {
            input.cached += 1;
            continue;
        }
        // libretro has no such kind, or didn't have this one before.
        match lr_name
            .and_then(|n| thumbnail_url(&opts.thumbnails_base, n, kind, &lr_game))
            .filter(|_| opts.refresh || !matches!(known, Some(None)))
        {
            Some(url) => {
                let mut urls = vec![url];
                urls.extend(
                    others.iter().filter_map(|o| {
                        lr_name.and_then(|n| thumbnail_url(&opts.thumbnails_base, n, kind, o))
                    }),
                );
                input.libretro.push((kind, urls));
            }
            None => input.missing.push(kind),
        }
    }
    let wants_art = !(input.libretro.is_empty() && input.missing.is_empty());
    if wants_art && let Some(lb) = lbdb {
        match crate::extdb::launchbox_lookup(lb, system, game, &opts.launchbox_images) {
            Ok(Some(found)) => input.launchbox = found.images,
            Ok(None) => {}
            Err(e) => input.errors.push(format!("LaunchBox: {e}")),
        }
    }
    for provider in Provider::available(keys, system) {
        let asked: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM art_lookup WHERE system_id = ?1 AND title = ?2 AND provider = ?3
               AND checked_at >= ?4)",
            params![system_id, game, provider.name(), ASK_AGAIN_SINCE],
            |r| r.get(0),
        )?;
        // One with text is asked once even when no picture is missing.
        if (!asked || opts.refresh) && (wants_art || provider.has_text()) {
            input.providers.push(provider);
        }
    }
    if input.providers.contains(&Provider::GameTdb) {
        let kinds: Vec<&str> = crate::artsources::gametdb_kinds().collect();
        input.serial = crate::artsources::serial_for(conn, system_id, game, &kinds)?;
    }
    if !input.providers.is_empty() {
        input.rom = crate::artsources::rom_for(conn, system_id, game)?;
        input.year = conn
            .query_row(
                "SELECT value FROM game_meta WHERE system_id = ?1 AND title = ?2 AND field = 'releaseyear'",
                params![system_id, game],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .and_then(|y| y.trim().parse().ok());
    }
    Ok(input)
}

/// The downloads for one game (no database), each kind from the first
/// source that has it: the free sources (the Arcade Database, GameTDB:
/// larger pictures than libretro's), libretro's thumbnails, LaunchBox,
/// then the keyed sources.
pub(super) fn fetch(sh: &Shared, input: GameIn) -> GameOut {
    let GameIn {
        game,
        libretro,
        missing,
        launchbox,
        providers,
        rom,
        year,
        serial,
        errors,
        ..
    } = input;
    let mut got: Vec<Got> = errors.into_iter().map(Got::Error).collect();
    // Every kind not kept yet: libretro's, and those it can't give.
    let mut wanted: Vec<ArtKind> = libretro.iter().map(|(k, _)| *k).chain(missing).collect();
    let job = crate::artsources::Job {
        system: sh.system,
        title: &game,
        rom,
        prefs: sh.prefs,
        language: sh.language,
        year,
        serial,
        video: sh.video,
    };
    // The IGDB token is cached for the whole process, so a session per
    // game is enough.
    let mut session = crate::artsources::Session::default();
    let (free, keyed): (Vec<_>, Vec<_>) = providers.into_iter().partition(|p| p.is_free());
    for provider in free {
        super::ask::ask(sh, provider, &job, &mut wanted, &mut got, &mut session);
    }
    'kinds: for (kind, urls) in libretro {
        if !wanted.contains(&kind) {
            continue;
        }
        for url in &urls {
            match image(url, sh.halted_hosts) {
                Ok(Some(data)) => {
                    if super::ask::keep(sh, &game, kind, &data, "libretro", url, &mut got) {
                        wanted.retain(|k| *k != kind);
                    }
                    continue 'kinds;
                }
                Ok(None) => {}
                // Not reached (offline, rate-limited): looked for again
                // next time, neither missing nor asked elsewhere now.
                Err(e) => {
                    got.push(Got::Error(e.to_string()));
                    wanted.retain(|k| *k != kind);
                    continue 'kinds;
                }
            }
        }
        got.push(Got::NotOnLibretro(kind));
    }
    // LaunchBox next (free, no key; downloaded by the user): only what is
    // still missing.
    for ArtFound { kind, url, .. } in launchbox {
        if !wanted.contains(&kind) {
            continue;
        }
        match image(&url, sh.halted_hosts) {
            Ok(Some(data)) => {
                if super::ask::keep(sh, &game, kind, &data, "launchbox", &url, &mut got) {
                    wanted.retain(|k| *k != kind);
                }
            }
            Ok(None) => {}
            Err(e) => got.push(Got::Error(format!("LaunchBox: {e}"))),
        }
    }
    for provider in keyed {
        super::ask::ask(sh, provider, &job, &mut wanted, &mut got, &mut session);
    }
    GameOut {
        game,
        got,
        missing: wanted.len(),
    }
}

/// Record what a worker found for a game.
pub(super) fn record_out(
    conn: &Connection,
    system_id: i64,
    out: &GameOut,
    report: &mut EnrichReport,
) -> Result<(), Error> {
    for g in &out.got {
        match g {
            Got::Saved {
                kind,
                path,
                source,
                url,
            } => {
                super::record(conn, system_id, &out.game, *kind, Some(path), source)?;
                conn.execute(
                    "UPDATE art SET url = ?4, chosen = 0 WHERE system_id = ?1 AND title = ?2 AND kind = ?3",
                    params![
                        system_id,
                        out.game,
                        kind.as_str(),
                        crate::artsources::without_credentials(url)
                    ],
                )?;
                report.art_saved += 1;
            }
            Got::NotOnLibretro(kind) => super::record(conn, system_id, &out.game, *kind, None, "libretro")?,
            Got::Asked(provider) => {
                conn.execute(
                    concat!(
                        "INSERT INTO art_lookup (system_id, title, provider, checked_at)
                         VALUES (?1, ?2, ?3, ",
                        crate::db::now!(),
                        ")
                         ON CONFLICT(system_id, title, provider) DO UPDATE SET checked_at = excluded.checked_at"
                    ),
                    params![system_id, out.game, provider.name()],
                )?;
            }
            Got::Fields(provider, fields) => {
                put_fields(conn, system_id, &out.game, fields, provider.name())?;
            }
            Got::Error(e) => report.errors.push(e.clone()),
        }
    }
    report.art_missing += out.missing;
    Ok(())
}

/// Where a game's picture of `kind` is kept.
pub(super) fn art_path(
    opts: &EnrichOptions,
    system: &systems::System,
    game: &str,
    kind: ArtKind,
    ext: &str,
) -> PathBuf {
    opts.art_dir
        .clone()
        .unwrap_or_else(|| opts.cache_dir.join(crate::layout::LOOSE_ART_DIR))
        .join(system.key)
        .join(kind.as_str())
        .join(format!("{}.{ext}", super::cache_stem(game)))
}

/// A picture, unless its host rate-limited this run (any worker); one that
/// does now is added. `None`: not there, or not a picture.
pub(super) fn image(url: &str, mut halted: &Mutex<HashSet<String>>) -> Result<Option<Vec<u8>>, Error> {
    let r = super::halting(&fetch::Request::image(url), &mut halted)?;
    Ok(r.filter(|d| image_ext(d).is_some()))
}
