//! IGDB (tier 3): a game by its title on the system's platforms, exact
//! names only (several told apart by release year): cover, artworks
//! (heroes), screenshots, and its summary and genres.

use super::{Bases, Found, Job, Session, credentials, json};
use crate::enrich::{ArtFound, ArtKind};
use crate::fetch::{self, Request};
use crate::settings::Keys;
use crate::{Error, wikidata};

pub(super) fn lookup(keys: &Keys, bases: &Bases, session: &mut Session, job: &Job) -> Result<Found, Error> {
    let (Some(id), Some(secret)) = (&keys.igdb_client_id, &keys.igdb_client_secret) else {
        return Ok(Found::default());
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
        "search \"{title}\"; fields name,summary,genres.name,first_release_date,\
         cover.image_id,screenshots.image_id,artworks.image_id; \
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
        return Ok(Found::default());
    };
    let games = json(&body)?;
    let Some(g) = pick(&games, job) else {
        return Ok(Found::default());
    };
    let image = |size: &str, id: &serde_json::Value| {
        id.as_str()
            .map(|i| format!("{}/{size}/{i}.jpg", bases.igdb_images))
    };
    let mut out = Vec::new();
    // Up to 1080 pixels high (`t_1080p` scales the original down, never
    // up), and a small one to preview.
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
    add(ArtKind::Boxart, "t_1080p", &g["cover"]["image_id"]);
    for a in g["artworks"].as_array().into_iter().flatten() {
        add(ArtKind::Hero, "t_1080p", &a["image_id"]);
    }
    for a in g["screenshots"].as_array().into_iter().flatten() {
        add(ArtKind::Snap, "t_1080p", &a["image_id"]);
    }
    Ok(Found {
        art: out,
        fields: fields(g),
    })
}

/// The game among IGDB's answers: the one of exactly this name, or of
/// those, the one released in the game's year; none when still several.
pub(super) fn pick<'a>(games: &'a serde_json::Value, job: &Job) -> Option<&'a serde_json::Value> {
    let want = wikidata::normalize(job.title);
    let named: Vec<&serde_json::Value> = games
        .as_array()
        .into_iter()
        .flatten()
        .filter(|g| g["name"].as_str().is_some_and(|n| wikidata::normalize(n) == want))
        .collect();
    match named[..] {
        [one] => Some(one),
        [] => None,
        _ => {
            let year = job.year?;
            let same: Vec<_> = named
                .into_iter()
                .filter(|g| g["first_release_date"].as_i64().map(year_of) == Some(year))
                .collect();
            match same[..] {
                [one] => Some(one),
                _ => None,
            }
        }
    }
}

/// The year of a Unix time (UTC).
fn year_of(secs: i64) -> i32 {
    // Howard Hinnant's days-from-civil, inverted.
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let y = yoe + era * 400 + i64::from(mp >= 10);
    y as i32
}

/// IGDB's summary and genres: filled only where earlier sources left
/// gaps ([`crate::enrich::put_field`]).
fn fields(g: &serde_json::Value) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    if let Some(s) = g["summary"].as_str().map(str::trim).filter(|s| !s.is_empty()) {
        out.push(("description", s.to_owned()));
    }
    let genres: Vec<&str> = g["genres"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x["name"].as_str())
        .collect();
    if !genres.is_empty() {
        out.push(("genre", genres.join(", ")));
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn unix_times_give_their_year() {
        assert_eq!(super::year_of(0), 1970);
        assert_eq!(super::year_of(766_627_200), 1994); // 1994-04-18
        assert_eq!(super::year_of(946_684_799), 1999);
        assert_eq!(super::year_of(946_684_800), 2000);
    }
}

#[cfg(test)]
mod pick_tests {
    use super::*;

    #[test]
    fn same_named_games_are_told_apart_by_year() {
        let games = serde_json::json!([
            {"name": "Prince of Persia", "first_release_date": 623_635_200},
            {"name": "Prince of Persia", "first_release_date": 1_207_180_800},
            {"name": "Prince of Persia: Sands"}
        ]);
        let system = crate::systems::by_key("SNES").unwrap();
        let job = |year| Job {
            system,
            title: "Prince of Persia (USA)",
            rom: None,
            prefs: &[],
            language: "en",
            year,
            serial: None,
            video: false,
        };
        assert!(pick(&games, &job(None)).is_none(), "two of that name, no year");
        let g = pick(&games, &job(Some(1989))).unwrap();
        assert_eq!(g["first_release_date"].as_i64(), Some(623_635_200));
        assert!(pick(&games, &job(Some(2001))).is_none());
    }
}
