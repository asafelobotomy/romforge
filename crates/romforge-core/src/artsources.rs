//! Milestone 9, second stage: art from sources that need the owner's keys
//! (house rule 10: ScreenScraper is tier 2, SteamGridDB and IGDB tier 3).
//! They only fill art libretro doesn't have.
//!
//! - ScreenScraper looks a game up by its ROM's hashes (the strongest
//!   match): 2D and 3D box, wheel logo, fanart, title screen, screenshot.
//! - SteamGridDB looks it up by title, accepting only one game with
//!   exactly that title (it knows no platforms): hero, logo, icon.
//! - IGDB looks it up by title and platform, exact titles only: cover,
//!   artwork (hero), screenshot.

use rusqlite::{Connection, OptionalExtension, params};

use crate::enrich::{ArtFound, ArtKind};
use crate::fetch::{self, Request};
use crate::settings::Keys;
use crate::systems::System;
use crate::{Error, wikidata};

mod credentials;
pub use credentials::{download_url, without_credentials};

/// Where each source lives (changed only in tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bases {
    pub screenscraper: String,
    pub steamgriddb: String,
    pub igdb_token: String,
    pub igdb: String,
    pub igdb_images: String,
}

impl Default for Bases {
    fn default() -> Self {
        Bases {
            screenscraper: "https://api.screenscraper.fr/api2".into(),
            steamgriddb: "https://www.steamgriddb.com/api/v2".into(),
            igdb_token: "https://id.twitch.tv/oauth2/token".into(),
            igdb: "https://api.igdb.com/v4".into(),
            igdb_images: "https://images.igdb.com/igdb/image/upload".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    ScreenScraper,
    SteamGridDb,
    Igdb,
}

impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Provider::ScreenScraper => "screenscraper",
            Provider::SteamGridDb => "steamgriddb",
            Provider::Igdb => "igdb",
        }
    }

    /// The sources usable for `system` with `keys`, in house rule 10 order.
    pub fn available(keys: &Keys, system: &System) -> Vec<Provider> {
        let mut out = Vec::new();
        if keys.screenscraper() && system.screenscraper.is_some() {
            out.push(Provider::ScreenScraper);
        }
        if keys.steamgriddb() {
            out.push(Provider::SteamGridDb);
        }
        if keys.igdb() && !system.igdb.is_empty() {
            out.push(Provider::Igdb);
        }
        out
    }
}

/// One ROM of a game, for hash lookups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RomRef {
    pub name: String,
    pub size: u64,
    pub crc32: String,
    pub md5: String,
    pub sha1: String,
}

/// The first matched (or hand-named) file of a game.
pub fn rom_for(conn: &Connection, system_id: i64, title: &str) -> Result<Option<RomRef>, Error> {
    let row = |r: &rusqlite::Row| -> rusqlite::Result<RomRef> {
        let path: String = r.get(0)?;
        let member: String = r.get(1)?;
        let shown = if member.is_empty() { path } else { member };
        Ok(RomRef {
            name: shown.rsplit('/').next().unwrap_or(&shown).to_owned(),
            size: r.get(2)?,
            crc32: r.get(3)?,
            md5: r.get(4)?,
            sha1: r.get(5)?,
        })
    };
    let hashed = conn
        .query_row(
            concat!(
                "SELECT f.path, f.member, f.size, f.crc32, f.md5, f.sha1
             FROM files f JOIN matches m ON m.file_id = f.file_id AND m.via IN ",
                crate::report::strong!(),
                "
             JOIN roms r ON r.rom_id = m.rom_id JOIN games g ON g.game_id = r.game_id
             JOIN dats d ON d.dat_id = g.dat_id
             WHERE d.system_id = ?1 AND g.name = ?2 AND f.status IN ",
                crate::report::holding!(),
                "
             ORDER BY f.size DESC LIMIT 1"
            ),
            params![system_id, title],
            row,
        )
        .optional()?;
    if hashed.is_some() {
        return Ok(hashed);
    }
    Ok(conn
        .query_row(
            "SELECT f.path, f.member, f.size, f.crc32, f.md5, f.sha1
             FROM files f JOIN manual_ids mi ON mi.sha1 = f.sha1 AND mi.size = f.size
             WHERE mi.system_id = ?1 AND mi.title = ?2 ORDER BY f.size DESC LIMIT 1",
            params![system_id, title],
            row,
        )
        .optional()?)
}

/// What one game needs looked up.
pub struct Job<'a> {
    pub system: &'a System,
    pub title: &'a str,
    pub rom: Option<RomRef>,
}

/// Remembers an IGDB token for the rest of a run.
#[derive(Default)]
pub struct Session {
    igdb_token: Option<String>,
}

/// Every image `provider` has for the job, the best of each kind first.
pub fn lookup(
    provider: Provider,
    keys: &Keys,
    bases: &Bases,
    session: &mut Session,
    job: &Job,
) -> Result<Vec<ArtFound>, Error> {
    match provider {
        Provider::ScreenScraper => screenscraper(keys, bases, job),
        Provider::SteamGridDb => steamgriddb(keys, bases, job),
        Provider::Igdb => igdb(keys, bases, session, job),
    }
}

fn json(body: &[u8]) -> Result<serde_json::Value, Error> {
    serde_json::from_slice(body).map_err(|e| Error::Http(format!("unexpected answer: {e}")))
}

fn screenscraper(keys: &Keys, bases: &Bases, job: &Job) -> Result<Vec<ArtFound>, Error> {
    let (Some(system_id), Some(rom)) = (job.system.screenscraper, &job.rom) else {
        return Ok(Vec::new());
    };
    let e = fetch::encode_segment;
    let opt = |v: &Option<String>| v.as_deref().map(e).unwrap_or_default();
    let url = format!(
        "{}/jeuInfos.php?devid={}&devpassword={}&softname=romforge&output=json&ssid={}&sspassword={}\
         &systemeid={system_id}&romtype=rom&crc={}&md5={}&sha1={}&romnom={}&romtaille={}",
        bases.screenscraper,
        opt(&keys.screenscraper_dev_id),
        opt(&keys.screenscraper_dev_password),
        opt(&keys.screenscraper_user),
        opt(&keys.screenscraper_password),
        rom.crc32.to_uppercase(),
        rom.md5,
        rom.sha1,
        e(&rom.name),
        rom.size
    );
    let Some(body) = fetch::get_optional(&url)? else {
        return Ok(Vec::new());
    };
    let v = json(&body)?;
    let medias = v["response"]["jeu"]["medias"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    // Prefer US / world / Europe / ScreenScraper's own / Japan versions.
    let rank = |r: &str| {
        ["us", "wor", "eu", "ss", "jp"]
            .iter()
            .position(|x| *x == r)
            .unwrap_or(9)
    };
    let mut all: Vec<(usize, ArtFound)> = Vec::new();
    for m in medias {
        let kind = match m["type"].as_str().unwrap_or_default() {
            "box-2D" => ArtKind::Boxart,
            "box-3D" => ArtKind::Box3d,
            "wheel" | "wheel-hd" => ArtKind::Logo,
            "fanart" => ArtKind::Hero,
            "sstitle" => ArtKind::Title,
            "ss" => ArtKind::Snap,
            _ => continue,
        };
        let Some(url) = m["url"].as_str() else { continue };
        let region = m["region"].as_str().unwrap_or("ss");
        // Its media URLs carry the credentials; they are put back only to
        // download ([`download_url`]).
        let url = without_credentials(url);
        let sep = if url.contains('?') { '&' } else { '?' };
        all.push((
            rank(region),
            ArtFound {
                kind,
                // ScreenScraper scales its media on request.
                thumb: Some(format!("{url}{sep}maxwidth=480")),
                url,
                label: Some(region.to_uppercase()),
            },
        ));
    }
    all.sort_by_key(|(r, _)| *r);
    Ok(all.into_iter().map(|(_, f)| f).collect())
}

fn steamgriddb(keys: &Keys, bases: &Bases, job: &Job) -> Result<Vec<ArtFound>, Error> {
    let Some(key) = &keys.steamgriddb_key else {
        return Ok(Vec::new());
    };
    let auth = vec![("Authorization", format!("Bearer {key}"))];
    let get = |url: String| -> Result<Option<serde_json::Value>, Error> {
        match fetch::request(&Request {
            url: &url,
            headers: auth.clone(),
            ..Request::default()
        })? {
            Some(b) => Ok(Some(json(&b)?)),
            None => Ok(None),
        }
    };
    let title = crate::view::search_title(job.title);
    let Some(found) = get(format!(
        "{}/search/autocomplete/{}",
        bases.steamgriddb,
        fetch::encode_segment(&title)
    ))?
    else {
        return Ok(Vec::new());
    };
    // It has no platforms to tell same-named games apart (a remake, a PC
    // port), so two of that title are none.
    let want = wikidata::normalize(job.title);
    let named: Vec<u64> = found["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|g| g["name"].as_str().is_some_and(|n| wikidata::normalize(n) == want))
        .filter_map(|g| g["id"].as_u64())
        .collect();
    let [id] = named[..] else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    // No grids: they are Steam library capsules people design, not the
    // game's box, so they never stand in for box art.
    for (kind, path) in [
        (ArtKind::Hero, format!("heroes/game/{id}")),
        (ArtKind::Logo, format!("logos/game/{id}")),
        (ArtKind::Icon, format!("icons/game/{id}")),
    ] {
        let Some(v) = get(format!("{}/{path}", bases.steamgriddb))? else {
            continue;
        };
        for x in v["data"].as_array().into_iter().flatten() {
            let Some(url) = x["url"].as_str() else { continue };
            let size = match (x["width"].as_u64(), x["height"].as_u64()) {
                (Some(w), Some(h)) => Some(format!("{w}×{h}")),
                _ => None,
            };
            let style = x["style"]
                .as_str()
                .filter(|s| *s != "alternate")
                .map(str::to_owned);
            let label = [style, size]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
            out.push(ArtFound {
                kind,
                url: url.to_owned(),
                thumb: x["thumb"].as_str().map(str::to_owned),
                label: (!label.is_empty()).then_some(label),
            });
        }
    }
    Ok(out)
}

fn igdb(keys: &Keys, bases: &Bases, session: &mut Session, job: &Job) -> Result<Vec<ArtFound>, Error> {
    let (Some(id), Some(secret)) = (&keys.igdb_client_id, &keys.igdb_client_secret) else {
        return Ok(Vec::new());
    };
    let cache_key = {
        use sha1::Digest as _;
        let h = crate::hash::hex(&sha1::Sha1::digest(secret.as_bytes()));
        format!("{}\t{id}\t{h}", bases.igdb_token)
    };
    if session.igdb_token.is_none() {
        session.igdb_token = credentials::cached_igdb_token(&cache_key, None);
    }
    if session.igdb_token.is_none() {
        let url = format!(
            "{}?client_id={}&client_secret={}&grant_type=client_credentials",
            bases.igdb_token,
            fetch::encode_segment(id),
            fetch::encode_segment(secret)
        );
        let body = fetch::request(&Request {
            url: &url,
            body: Some(String::new()),
            ..Request::default()
        })?
        .ok_or_else(|| Error::Http("IGDB: no token".into()))?;
        let answer = json(&body)?;
        let token = answer["access_token"]
            .as_str()
            .ok_or_else(|| Error::Http("IGDB: no token in the answer".into()))?
            .to_owned();
        // Twitch says how long it lasts; without that, this run only.
        session.igdb_token = match answer["expires_in"].as_u64() {
            Some(secs) => credentials::cached_igdb_token(&cache_key, Some((token, secs))),
            None => Some(token),
        };
    }
    let token = session.igdb_token.clone().unwrap_or_default();
    let title = crate::view::search_title(job.title).replace('"', "");
    let platforms: Vec<String> = job.system.igdb.iter().map(|p| p.to_string()).collect();
    let query = format!(
        "search \"{title}\"; fields name,cover.image_id,screenshots.image_id,artworks.image_id; \
         where platforms = ({}); limit 10;",
        platforms.join(",")
    );
    let Some(body) = fetch::request(&Request {
        url: &format!("{}/games", bases.igdb),
        headers: vec![
            ("Client-ID", id.clone()),
            ("Authorization", format!("Bearer {token}")),
        ],
        body: Some(query),
        ..Request::default()
    })?
    else {
        return Ok(Vec::new());
    };
    let want = wikidata::normalize(job.title);
    let games = json(&body)?;
    let Some(g) = games
        .as_array()
        .into_iter()
        .flatten()
        .find(|g| g["name"].as_str().is_some_and(|n| wikidata::normalize(n) == want))
    else {
        return Ok(Vec::new());
    };
    let image = |size: &str, id: &serde_json::Value| {
        id.as_str()
            .map(|i| format!("{}/{size}/{i}.jpg", bases.igdb_images))
    };
    let mut out = Vec::new();
    // The largest sizes IGDB serves without scaling past the original
    // (cover 528x748, artwork 1920x1080, screenshot 1280x720), and a small
    // one to preview.
    let mut add = |kind: ArtKind, size: &str, id: &serde_json::Value| {
        if let Some(url) = image(size, id) {
            out.push(ArtFound {
                kind,
                url,
                thumb: image("t_screenshot_med", id),
                label: None,
            });
        }
    };
    add(ArtKind::Boxart, "t_cover_big_2x", &g["cover"]["image_id"]);
    for a in g["artworks"].as_array().into_iter().flatten() {
        add(ArtKind::Hero, "t_1080p", &a["image_id"]);
    }
    for a in g["screenshots"].as_array().into_iter().flatten() {
        add(ArtKind::Snap, "t_screenshot_huge", &a["image_id"]);
    }
    Ok(out)
}
