//! Milestone 3: metadata and art for the games you have, cached locally.
//!
//! Both come from libretro, needing no account:
//! - metadata from libretro-database's `metadat/<field>/<system>.dat`
//!   (genre, developer, publisher, franchise, release year and month, max
//!   players; some entries also carry a description or serial);
//! - art from `thumbnails.libretro.com/<system>/Named_<kind>s/<game>.png`
//!   (box art, title screens, in-game snaps), saved under the cache folder;
//!   what libretro lacks (and hero, logo, icon, 3D box) from the keyed
//!   sources in `artsources.rs`, when the owner entered keys.
//!
//! Records are matched to a game by its exact DAT name, else by the CRC32
//! of one of its ROMs within that system's file. Only games with a matched
//! file are looked up. Nothing is written next to the ROMs.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::{Error, dat, fetch, systems};

pub const THUMBNAILS_BASE: &str = "https://thumbnails.libretro.com";

/// The game's name in libretro's database, kept when it differs from the
/// DAT's (matched by CRC): libretro's thumbnails are named by it.
pub const LIBRETRO_NAME: &str = "libretro_name";

/// The metadata files fetched per system.
pub const FIELDS: &[&str] = &[
    "genre",
    "developer",
    "publisher",
    "franchise",
    "releaseyear",
    "releasemonth",
    "maxusers",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtKind {
    Boxart,
    Title,
    Snap,
    /// A wide banner (hero) image.
    Hero,
    Logo,
    Icon,
    /// A 3D render of the box.
    Box3d,
}

impl ArtKind {
    pub const ALL: [ArtKind; 7] = [
        ArtKind::Boxart,
        ArtKind::Title,
        ArtKind::Snap,
        ArtKind::Hero,
        ArtKind::Logo,
        ArtKind::Icon,
        ArtKind::Box3d,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ArtKind::Boxart => "boxart",
            ArtKind::Title => "title",
            ArtKind::Snap => "snap",
            ArtKind::Hero => "hero",
            ArtKind::Logo => "logo",
            ArtKind::Icon => "icon",
            ArtKind::Box3d => "box3d",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.as_str().eq_ignore_ascii_case(s.trim()))
    }

    /// libretro's thumbnail folder; it has only these three kinds.
    fn folder(self) -> Option<&'static str> {
        match self {
            ArtKind::Boxart => Some("Named_Boxarts"),
            ArtKind::Title => Some("Named_Titles"),
            ArtKind::Snap => Some("Named_Snaps"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrichOptions {
    /// Where art is saved (`<cache>/art/<system>/<kind>/<game>.png`).
    pub cache_dir: PathBuf,
    pub art: Vec<ArtKind>,
    /// Only this system key.
    pub system: Option<String>,
    /// Only the game with this title (with `system`, one game: Search
    /// fetches what you asked for right after you pick a result).
    pub title: Option<String>,
    /// Fetch again what was fetched (or found missing) before.
    pub refresh: bool,
    /// libretro-database's `metadat` URL ([`fetch::LIBRETRO_BASE`]).
    pub metadata_base: String,
    /// The thumbnail server ([`THUMBNAILS_BASE`]).
    pub thumbnails_base: String,
    /// Wikidata's SPARQL endpoint ([`crate::wikidata::ENDPOINT`]); `None`
    /// skips Wikidata.
    pub wikidata: Option<String>,
    /// Where the keyed art sources live.
    pub sources: crate::artsources::Bases,
    /// The folder of databases the user downloaded (OpenVGDB, LaunchBox);
    /// `None`: the library's own (`databases/` next to it), if any.
    pub extdb: Option<PathBuf>,
    /// LaunchBox's image server ([`crate::extdb::Bases`]).
    pub launchbox_images: String,
}

impl EnrichOptions {
    /// Box art only, from libretro, saved under `cache_dir`.
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
            art: vec![ArtKind::Boxart],
            system: None,
            title: None,
            refresh: false,
            metadata_base: fetch::LIBRETRO_BASE.to_owned(),
            thumbnails_base: THUMBNAILS_BASE.to_owned(),
            wikidata: Some(crate::wikidata::ENDPOINT.to_owned()),
            sources: crate::artsources::Bases::default(),
            extdb: None,
            launchbox_images: crate::extdb::Bases::default().launchbox_images,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct EnrichReport {
    pub systems: usize,
    pub games: usize,
    pub games_with_metadata: usize,
    pub art_saved: usize,
    pub art_missing: usize,
    pub art_cached: usize,
    /// Wikidata matching (when it ran).
    pub wikidata: Option<crate::wikidata::WikidataReport>,
    /// Games given fields by the downloaded databases (OpenVGDB, LaunchBox).
    pub extdb_games: usize,
    pub errors: Vec<String>,
}

/// What [`enrich`] is doing, for progress output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrichEvent {
    /// Wikidata: a platform being matched, or "details" / "series".
    Wikidata { step: &'static str },
    Metadata {
        system: &'static str,
        field: &'static str,
    },
    Art {
        system: &'static str,
        game: String,
        kind: ArtKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GameInfo {
    pub system: String,
    pub title: String,
    pub fields: BTreeMap<String, String>,
    pub art: BTreeMap<ArtKind, PathBuf>,
}

/// `$ROMFORGE_CACHE`, else `$XDG_CACHE_HOME/romforge`, else
/// `~/.cache/romforge`.
pub fn default_cache_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("ROMFORGE_CACHE") {
        return PathBuf::from(p);
    }
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("romforge")
}

/// libretro's name for a system (its database and thumbnail folders).
pub fn libretro_name(system: &systems::System) -> Option<&'static str> {
    system.nointro.first().or(system.dat_names.first()).copied()
}

/// libretro's thumbnail file name: `&*/:`<>?\|"` become `_`.
pub fn thumbnail_file(game: &str) -> String {
    let name: String = game
        .chars()
        .map(|c| if "&*/:`<>?\\|\"".contains(c) { '_' } else { c })
        .collect();
    format!("{name}.png")
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
    // Games you named in Search have no DAT ROMs (so no CRCs), but get
    // metadata and art by title like any other.
    let mut named = conn.prepare(
        "SELECT DISTINCT m.system_id, m.title FROM manual_ids m
         JOIN files f ON f.sha1 = m.sha1 AND f.size = m.size AND f.status = 'manual'
         WHERE ?1 IS NULL OR m.system_id = ?1",
    )?;
    for row in named.query_map([only], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
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

/// Fetch metadata and art for the games you have.
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
    // Databases the user downloaded (milestone 11), if any.
    let ovgdb = opts
        .extdb
        .as_deref()
        .and_then(|d| crate::extdb::open(crate::extdb::ExtDb::OpenVgdb, d));
    let lbdb = opts
        .extdb
        .as_deref()
        .and_then(|d| crate::extdb::open(crate::extdb::ExtDb::LaunchBox, d));
    let mut session = crate::artsources::Session::default();
    // Sources that said "slow down" aren't asked again this run.
    let mut halted: Vec<crate::artsources::Provider> = Vec::new();
    for (system_id, games) in &by_system {
        let Some(system) = systems::by_id(*system_id) else {
            continue;
        };
        let Some(lr_name) = libretro_name(system) else {
            continue;
        };

        // Metadata: one file per field; a missing file is fine.
        if opts.refresh || games_without_meta(conn, *system_id, games)? {
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
                let body = match fetch::get_optional(&url) {
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
                        f.extend(rec.fields.iter().cloned());
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
                let mut put = tx.prepare(
                    "INSERT INTO game_meta (system_id, title, field, value) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(system_id, title, field) DO UPDATE SET value = excluded.value, source = NULL",
                )?;
                for (game, fields) in &found {
                    for (k, v) in fields {
                        put.execute(params![system_id, game, k, v])?;
                    }
                }
                let mut checked = tx.prepare(
                    "INSERT INTO meta_checked (system_id, title, checked_at)
                     VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                     ON CONFLICT(system_id, title) DO UPDATE SET checked_at = excluded.checked_at",
                )?;
                // After a failed download, look again next time.
                if !fetch_failed {
                    for (game, _) in games {
                        checked.execute(params![system_id, game])?;
                    }
                }
            }
            tx.commit()?;
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

        // Art: libretro first (house rule 10, tier 1); the keyed sources
        // fill what it doesn't have.
        for (game, _) in games {
            let lr_game: Option<String> = conn
                .query_row(
                    "SELECT value FROM game_meta WHERE system_id = ?1 AND title = ?2 AND field = ?3",
                    params![system_id, game, LIBRETRO_NAME],
                    |r| r.get(0),
                )
                .optional()?;
            let lr_game = lr_game.as_deref().unwrap_or(game);
            let mut missing: Vec<ArtKind> = Vec::new();
            for &kind in &opts.art {
                let known: Option<Option<String>> = conn
                    .query_row(
                        "SELECT path FROM art WHERE system_id = ?1 AND title = ?2 AND kind = ?3",
                        params![system_id, game, kind.as_str()],
                        |r| r.get(0),
                    )
                    .optional()?;
                let still_there = matches!(&known, Some(Some(p)) if Path::new(p).exists());
                if still_there && !opts.refresh {
                    report.art_cached += 1;
                    continue;
                }
                // libretro has no such kind, or didn't have this one before.
                let Some(url) = thumbnail_url(&opts.thumbnails_base, lr_name, kind, lr_game)
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
                match fetch::get_optional(&url) {
                    Ok(Some(data)) if image_ext(&data).is_some() => {
                        match keep(conn, opts, system, *system_id, game, kind, &data, "libretro") {
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
                        for (kind, url) in found.images {
                            if !missing.contains(&kind) {
                                continue;
                            }
                            progress(EnrichEvent::Art {
                                system: system.key,
                                game: (*game).clone(),
                                kind,
                            });
                            match fetch::get_optional(&url) {
                                Ok(Some(data)) if image_ext(&data).is_some() => {
                                    match keep(conn, opts, system, *system_id, game, kind, &data, "launchbox")
                                    {
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
            let providers: Vec<_> = crate::artsources::Provider::available(&keys, system)
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
                match crate::artsources::lookup(provider, &keys, &opts.sources, &mut session, &job) {
                    Ok(found) => {
                        for (kind, url) in found {
                            if !missing.contains(&kind) {
                                continue;
                            }
                            progress(EnrichEvent::Art {
                                system: system.key,
                                game: (*game).clone(),
                                kind,
                            });
                            match fetch::get_optional(&url) {
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
                            "INSERT INTO art_lookup (system_id, title, provider, checked_at)
                             VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                             ON CONFLICT(system_id, title, provider) DO UPDATE SET checked_at = excluded.checked_at",
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
    }
    // Wikidata after libretro, so a release year is there to tell apart
    // games of the same name.
    if let Some(endpoint) = &opts.wikidata {
        match crate::wikidata::enrich(conn, endpoint, &have, opts.refresh, |step| {
            progress(EnrichEvent::Wikidata { step })
        }) {
            Ok(w) => report.wikidata = Some(w),
            Err(e) => report.errors.push(format!("Wikidata: {e}")),
        }
    }
    // Last, the downloaded databases fill fields nothing else had:
    // OpenVGDB by hash first, then LaunchBox by title.
    if ovgdb.is_some() || lbdb.is_some() {
        for (system_id, game) in have.keys() {
            let Some(system) = systems::by_id(*system_id) else {
                continue;
            };
            let mut found: Vec<(&'static str, &'static str, String)> = Vec::new();
            if let Some(db) = &ovgdb {
                match crate::extdb::openvgdb_lookup(db, &game_sha1s(conn, *system_id, game)?) {
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
    Ok(report)
}

/// The SHA1s of a game's ROMs (from its DATs) or of the files you named it.
fn game_sha1s(conn: &Connection, system_id: i64, title: &str) -> Result<Vec<String>, Error> {
    Ok(conn
        .prepare_cached(
            // From the system's DATs down (games have no index by name).
            "SELECT r.sha1 FROM dats d CROSS JOIN games g ON g.dat_id = d.dat_id
               CROSS JOIN roms r ON r.game_id = g.game_id
             WHERE d.system_id = ?1 AND g.name = ?2 AND +r.sha1 IS NOT NULL
             UNION SELECT sha1 FROM manual_ids WHERE system_id = ?1 AND title = ?2",
        )?
        .query_map(params![system_id, title], |r| r.get(0))?
        .collect::<Result<_, _>>()?)
}

/// Whether some game's metadata was never looked up (it may be newly
/// scanned).
fn games_without_meta(
    conn: &Connection,
    system_id: i64,
    games: &[(&String, &Vec<String>)],
) -> Result<bool, Error> {
    let mut stmt = conn
        .prepare_cached("SELECT EXISTS (SELECT 1 FROM meta_checked WHERE system_id = ?1 AND title = ?2)")?;
    for (game, _) in games {
        if !stmt.query_row(params![system_id, game], |r| r.get::<_, bool>(0))? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Save an image for a game and record it.
#[allow(clippy::too_many_arguments)]
fn keep(
    conn: &Connection,
    opts: &EnrichOptions,
    system: &systems::System,
    system_id: i64,
    game: &str,
    kind: ArtKind,
    data: &[u8],
    source: &str,
) -> Result<(), Error> {
    let ext = image_ext(data).unwrap_or("png");
    let stem = thumbnail_file(game);
    let stem = stem.strip_suffix(".png").unwrap_or(&stem);
    let path = opts
        .cache_dir
        .join("art")
        .join(system.key)
        .join(kind.as_str())
        .join(format!("{stem}.{ext}"));
    save(&path, data).map_err(|e| Error::Http(format!("{}: {e}", path.display())))?;
    record(conn, system_id, game, kind, Some(&path.to_string_lossy()), source)
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
        "INSERT INTO art (system_id, title, kind, path, checked_at, source)
         VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), ?5)
         ON CONFLICT(system_id, title, kind) DO UPDATE SET path = excluded.path,
           checked_at = excluded.checked_at, source = excluded.source",
        params![system_id, game, kind.as_str(), path, source],
    )?;
    Ok(())
}

/// Write `data` to `path` via a temporary file, so a crash never leaves half
/// an image under the real name.
fn save(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

/// Metadata and cached art for games whose name contains `query`
/// (case-insensitive; an exact name comes first).
pub fn info(conn: &Connection, query: &str) -> Result<Vec<GameInfo>, Error> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT s.key, t.system_id, t.title FROM (
             SELECT system_id, title FROM game_meta UNION SELECT system_id, title FROM art
         ) t JOIN systems s ON s.system_id = t.system_id
         WHERE t.title LIKE '%' || ?2 || '%' ESCAPE '\\'
         ORDER BY t.title <> ?1, t.title LIMIT 50",
    )?;
    let hits: Vec<(String, i64, String)> = stmt
        .query_map(params![query, crate::like_escape(query)], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<Result<_, _>>()?;
    let mut fields =
        conn.prepare("SELECT field, value FROM game_meta WHERE system_id = ?1 AND title = ?2")?;
    let mut art =
        conn.prepare("SELECT kind, path FROM art WHERE system_id = ?1 AND title = ?2 AND path IS NOT NULL")?;
    let mut out = Vec::new();
    for (key, system_id, title) in hits {
        let f: BTreeMap<String, String> = fields
            .query_map(params![system_id, title], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        let a: BTreeMap<ArtKind, PathBuf> = art
            .query_map(params![system_id, title], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .filter_map(|row| {
                let (k, p) = row.ok()?;
                Some((ArtKind::parse(&k)?, PathBuf::from(p)))
            })
            .collect();
        out.push(GameInfo {
            system: key,
            title,
            fields: f,
            art: a,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_names_and_urls() {
        assert_eq!(
            thumbnail_file("Pokemon - Ruby & Sapphire: X?"),
            "Pokemon - Ruby _ Sapphire_ X_.png"
        );
        let snes = systems::by_key("SNES").unwrap();
        assert_eq!(
            libretro_name(snes),
            Some("Nintendo - Super Nintendo Entertainment System")
        );
        assert_eq!(
            thumbnail_url(
                THUMBNAILS_BASE,
                libretro_name(snes).unwrap(),
                ArtKind::Boxart,
                "Super Mario World (USA)"
            )
            .as_deref(),
            Some(
                "https://thumbnails.libretro.com/Nintendo%20-%20Super%20Nintendo%20Entertainment%20System/Named_Boxarts/Super%20Mario%20World%20(USA).png"
            )
        );
        assert_eq!(
            thumbnail_url(THUMBNAILS_BASE, "x", ArtKind::Hero, "y"),
            None,
            "libretro has no heroes"
        );
        assert_eq!(image_ext(b"\xff\xd8\xff\xe0"), Some("jpg"));
        assert_eq!(image_ext(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(image_ext(b"<html>"), None);
        let psx = systems::by_key("PlayStation").unwrap();
        assert_eq!(libretro_name(psx), Some("Sony - PlayStation"));
    }
}
