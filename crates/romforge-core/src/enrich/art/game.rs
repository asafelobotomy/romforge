//! One game's art: what the database says first (on the calling thread),
//! the downloads on a worker (libretro, then LaunchBox, then the keyed
//! sources, in that order: house rule 10), then what was found recorded
//! (on the calling thread again).

use std::collections::HashSet;
use std::sync::Mutex;

use super::super::*;
use super::{image_ext, save, thumbnail_url};
use crate::artsources::{Provider, RomRef};

/// What a worker needs for one game, read beforehand.
pub(super) struct GameIn {
    pub game: String,
    /// libretro's thumbnails to try, by kind.
    libretro: Vec<(ArtKind, String)>,
    /// Kinds libretro can't give (or didn't before): for the others.
    missing: Vec<ArtKind>,
    /// LaunchBox's pictures of the game (from the downloaded database).
    launchbox: Vec<ArtFound>,
    /// Keyed sources to ask (not asked before, unless refreshing).
    providers: Vec<Provider>,
    rom: Option<RomRef>,
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
    lbdb: &Option<Connection>,
) -> Result<GameIn, Error> {
    let lr_game = libretro_game(conn, system, system_id, game)?;
    let mut input = GameIn {
        game: game.to_owned(),
        libretro: Vec::new(),
        missing: Vec::new(),
        launchbox: Vec::new(),
        providers: Vec::new(),
        rom: None,
        errors: Vec::new(),
        cached: 0,
    };
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
            input.cached += 1;
            continue;
        }
        // libretro has no such kind, or didn't have this one before.
        match lr_name
            .and_then(|n| thumbnail_url(&opts.thumbnails_base, n, kind, &lr_game))
            .filter(|_| opts.refresh || !matches!(known, Some(None)))
        {
            Some(url) => input.libretro.push((kind, url)),
            None => input.missing.push(kind),
        }
    }
    if input.libretro.is_empty() && input.missing.is_empty() {
        return Ok(input);
    }
    if let Some(lb) = lbdb {
        match crate::extdb::launchbox_lookup(lb, system, game, &opts.launchbox_images) {
            Ok(Some(found)) => input.launchbox = found.images,
            Ok(None) => {}
            Err(e) => input.errors.push(format!("LaunchBox: {e}")),
        }
    }
    for provider in Provider::available(keys, system) {
        let asked: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM art_lookup WHERE system_id = ?1 AND title = ?2 AND provider = ?3)",
            params![system_id, game, provider.name()],
            |r| r.get(0),
        )?;
        if !asked || opts.refresh {
            input.providers.push(provider);
        }
    }
    if !input.providers.is_empty() {
        input.rom = crate::artsources::rom_for(conn, system_id, game)?;
    }
    Ok(input)
}

/// The downloads for one game (no database): what libretro has, then
/// LaunchBox and the keyed sources for what is still missing.
pub(super) fn fetch(sh: &Shared, input: GameIn) -> GameOut {
    let GameIn {
        game,
        libretro,
        mut missing,
        launchbox,
        providers,
        rom,
        errors,
        ..
    } = input;
    let mut got: Vec<Got> = errors.into_iter().map(Got::Error).collect();
    let take = |kind: ArtKind, data: &[u8], source: &str, url: &str, got: &mut Vec<Got>| -> bool {
        let ext = image_ext(data).unwrap_or("png");
        let path = art_path(sh.opts, sh.system, &game, kind, ext);
        match save(&path, data) {
            Ok(()) => {
                got.push(Got::Saved {
                    kind,
                    path: path.to_string_lossy().into_owned(),
                    source: source.to_owned(),
                    url: url.to_owned(),
                });
                true
            }
            Err(e) => {
                got.push(Got::Error(format!("{}: {e}", path.display())));
                false
            }
        }
    };
    for (kind, url) in libretro {
        match image(&url, sh.halted_hosts) {
            Ok(Some(data)) => {
                if !take(kind, &data, "libretro", &url, &mut got) {
                    missing.push(kind);
                }
            }
            Ok(None) => {
                got.push(Got::NotOnLibretro(kind));
                missing.push(kind);
            }
            Err(e) => got.push(Got::Error(e.to_string())),
        }
    }
    // LaunchBox next (free, no key; downloaded by the user): only what is
    // still missing.
    for ArtFound { kind, url, .. } in launchbox {
        if !missing.contains(&kind) {
            continue;
        }
        match image(&url, sh.halted_hosts) {
            Ok(Some(data)) => {
                if take(kind, &data, "launchbox", &url, &mut got) {
                    missing.retain(|k| *k != kind);
                }
            }
            Ok(None) => {}
            Err(e) => got.push(Got::Error(format!("LaunchBox: {e}"))),
        }
    }
    // Keyed sources, each asked once per game unless refreshing. The IGDB
    // token is cached for the whole process, so a session per game is enough.
    let mut session = crate::artsources::Session::default();
    for provider in providers {
        if missing.is_empty() {
            break;
        }
        if sh
            .halted
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&provider)
        {
            continue;
        }
        let job = crate::artsources::Job {
            system: sh.system,
            title: &game,
            rom: rom.clone(),
        };
        match crate::artsources::lookup(provider, sh.keys, &sh.opts.sources, &mut session, &job) {
            Ok(found) => {
                for ArtFound { kind, url, .. } in found {
                    if !missing.contains(&kind) {
                        continue;
                    }
                    let get =
                        crate::artsources::download_url(provider.name(), &url, sh.keys, &sh.opts.sources);
                    match image(&get, sh.halted_hosts) {
                        Ok(Some(data)) => {
                            if take(kind, &data, provider.name(), &url, &mut got) {
                                missing.retain(|k| *k != kind);
                            }
                        }
                        Ok(None) => {}
                        Err(e) => got.push(Got::Error(e.to_string())),
                    }
                }
                got.push(Got::Asked(provider));
            }
            Err(e) => {
                if fetch::is_rate_limited(&e) {
                    sh.halted.lock().unwrap_or_else(|e| e.into_inner()).push(provider);
                }
                got.push(Got::Error(format!("{}: {e}", provider.name())));
            }
        }
    }
    GameOut {
        game,
        got,
        missing: missing.len(),
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
fn image(url: &str, mut halted: &Mutex<HashSet<String>>) -> Result<Option<Vec<u8>>, Error> {
    let r = super::halting(&fetch::Request::image(url), &mut halted)?;
    Ok(r.filter(|d| image_ext(d).is_some()))
}
