//! Milestone 3: metadata and art for the games you have, cached locally.
//!
//! Both come from libretro, needing no account:
//! - metadata from libretro-database's `metadat/<field>/<system>.dat`
//!   (genre, developer, publisher, franchise, release year and month, max
//!   players; some entries also carry a description or serial);
//! - art from `thumbnails.libretro.com/<system>/Named_<kind>s/<game>.png`
//!   (box art, title screens, in-game snaps), saved under the cache folder.
//!
//! Records are matched to a game by its exact DAT name, else by the CRC32
//! of one of its ROMs within that system's file. Only games with a matched
//! file are looked up. Nothing is written next to the ROMs.
//! ScreenScraper and IGDB need personal API keys; see PLAN.md.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::{Error, dat, fetch, systems};

pub const THUMBNAILS_BASE: &str = "https://thumbnails.libretro.com";

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
}

impl ArtKind {
    pub const ALL: [ArtKind; 3] = [ArtKind::Boxart, ArtKind::Title, ArtKind::Snap];

    pub fn as_str(self) -> &'static str {
        match self {
            ArtKind::Boxart => "boxart",
            ArtKind::Title => "title",
            ArtKind::Snap => "snap",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.as_str().eq_ignore_ascii_case(s.trim()))
    }

    fn folder(self) -> &'static str {
        match self {
            ArtKind::Boxart => "Named_Boxarts",
            ArtKind::Title => "Named_Titles",
            ArtKind::Snap => "Named_Snaps",
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
    /// Fetch again what was fetched (or found missing) before.
    pub refresh: bool,
    /// libretro-database's `metadat` URL ([`fetch::LIBRETRO_BASE`]).
    pub metadata_base: String,
    /// The thumbnail server ([`THUMBNAILS_BASE`]).
    pub thumbnails_base: String,
}

impl EnrichOptions {
    /// Box art only, from libretro, saved under `cache_dir`.
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
            art: vec![ArtKind::Boxart],
            system: None,
            refresh: false,
            metadata_base: fetch::LIBRETRO_BASE.to_owned(),
            thumbnails_base: THUMBNAILS_BASE.to_owned(),
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
    pub errors: Vec<String>,
}

/// What [`enrich`] is doing, for progress output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrichEvent {
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

pub fn thumbnail_url(base: &str, system_name: &str, kind: ArtKind, game: &str) -> String {
    format!(
        "{base}/{}/{}/{}",
        fetch::encode_segment(system_name),
        kind.folder(),
        fetch::encode_segment(&thumbnail_file(game))
    )
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
    let have = games_you_have(conn, only)?;
    let mut by_system: BTreeMap<i64, Vec<(&String, &Vec<String>)>> = BTreeMap::new();
    for ((system, game), crcs) in &have {
        by_system.entry(*system).or_default().push((game, crcs));
    }
    let mut report = EnrichReport {
        systems: by_system.len(),
        games: have.len(),
        ..EnrichReport::default()
    };

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
                        found.entry(game).or_default().extend(rec.fields.iter().cloned());
                    }
                }
            }
            let tx = conn.transaction()?;
            {
                let mut put = tx.prepare(
                    "INSERT INTO game_meta (system_id, title, field, value) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(system_id, title, field) DO UPDATE SET value = excluded.value",
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

        // Art.
        for (game, _) in games {
            for &kind in &opts.art {
                let known: Option<Option<String>> = conn
                    .query_row(
                        "SELECT path FROM art WHERE system_id = ?1 AND title = ?2 AND kind = ?3",
                        params![system_id, game, kind.as_str()],
                        |r| r.get(0),
                    )
                    .optional()?;
                let still_there = matches!(&known, Some(Some(p)) if Path::new(p).exists());
                if !opts.refresh && (still_there || matches!(known, Some(None))) {
                    report.art_cached += 1;
                    continue;
                }
                progress(EnrichEvent::Art {
                    system: system.key,
                    game: (*game).clone(),
                    kind,
                });
                let url = thumbnail_url(&opts.thumbnails_base, lr_name, kind, game);
                let path = match fetch::get_optional(&url) {
                    Ok(Some(png)) if png.starts_with(b"\x89PNG") => {
                        let path = opts
                            .cache_dir
                            .join("art")
                            .join(system.key)
                            .join(kind.as_str())
                            .join(thumbnail_file(game));
                        if let Err(e) = save(&path, &png) {
                            report.errors.push(format!("{}: {e}", path.display()));
                            continue;
                        }
                        report.art_saved += 1;
                        Some(path.to_string_lossy().into_owned())
                    }
                    Ok(_) => {
                        report.art_missing += 1;
                        None
                    }
                    Err(e) => {
                        report.errors.push(e.to_string());
                        continue;
                    }
                };
                conn.execute(
                    "INSERT INTO art (system_id, title, kind, path, checked_at)
                     VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                     ON CONFLICT(system_id, title, kind) DO UPDATE SET path = excluded.path,
                       checked_at = excluded.checked_at",
                    params![system_id, game, kind.as_str(), path],
                )?;
            }
        }
    }
    Ok(report)
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

/// Write `data` to `path` via a temporary file, so a crash never leaves half
/// an image under the real name.
fn save(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("png.part");
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
            ),
            "https://thumbnails.libretro.com/Nintendo%20-%20Super%20Nintendo%20Entertainment%20System/Named_Boxarts/Super%20Mario%20World%20(USA).png"
        );
        let psx = systems::by_key("PlayStation").unwrap();
        assert_eq!(libretro_name(psx), Some("Sony - PlayStation"));
    }
}
