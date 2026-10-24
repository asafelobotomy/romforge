//! The Arcade Database (adb.arcadeitalia.net, by motoschifo): an arcade
//! set by its name, free and keyless. One `query_mame` call per set (the
//! service takes one at a time, from one connection: `fetch` keeps to
//! that) gives its pictures, its facts, a video snap and its history from
//! arcade-history.com, which is shown with its copyright line. Its terms:
//! cite the source ("Scraping using Arcade Database by motoschifo").

use serde_json::Value;

use super::{Bases, Found, Job, json};
use crate::Error;
use crate::enrich::{ArtFound, ArtKind};
use crate::fetch;

pub(super) fn lookup(bases: &Bases, job: &Job) -> Result<Found, Error> {
    if !job.system.arcade {
        return Ok(Found::default());
    }
    let url = format!(
        "{}/service_scraper.php?ajax=query_mame&game_name={}",
        bases.adb,
        fetch::encode_segment(job.title)
    );
    let Some(body) = fetch::get_small_optional(&url)? else {
        return Ok(Found::default());
    };
    let v = json(&body)?;
    let set = v["result"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|g| g["game_name"].as_str() == Some(job.title));
    Ok(set.map(|g| read(g, job.video)).unwrap_or_default())
}

/// A set's manual (a PDF), if the Arcade Database has one: its
/// `query_mame_media` call, asked only when the Manual tab opens.
pub fn manual_url(bases: &Bases, set: &str) -> Result<Option<String>, Error> {
    let url = format!(
        "{}/service_scraper.php?ajax=query_mame_media&game_name={}",
        bases.adb,
        fetch::encode_segment(set)
    );
    let Some(body) = fetch::get_small_optional(&url)? else {
        return Ok(None);
    };
    Ok(json(&body)?["result"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|g| g["game_name"].as_str() == Some(set))
        .and_then(|g| g["url_manual"].as_str())
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(str::to_owned))
}

/// One set's answer; its video only when `video` (Settings).
pub(super) fn read(g: &Value, video: bool) -> Found {
    let text = |k: &str| g[k].as_str().map(str::trim).filter(|s| !s.is_empty());
    let mut art = Vec::new();
    for (kind, key) in [
        (ArtKind::Snap, "url_image_ingame"),
        (ArtKind::Title, "url_image_title"),
        (ArtKind::Marquee, "url_image_marquee"),
        (ArtKind::Flyer, "url_image_flyer"),
        (ArtKind::Cabinet, "url_image_cabinet"),
    ] {
        if let Some(url) = text(key) {
            art.push(ArtFound {
                kind,
                thumb: None,
                url: url.to_owned(),
                label: Some("Arcade Database".into()),
            });
        }
    }
    if video && let Some(url) = text("url_video_shortplay") {
        art.push(ArtFound::new(ArtKind::Video, url.to_owned()));
    }
    let mut fields: Vec<(&'static str, String)> = Vec::new();
    let mut put = |k: &'static str, v: Option<&str>| {
        if let Some(v) = v {
            fields.push((k, v.to_owned()));
        }
    };
    if let Some(history) = text("history") {
        put("description", Some(history));
        put(
            "description_licence",
            text("history_copyright_short").or(Some("arcade-history.com")),
        );
        put("description_url", text("url"));
    }
    put("genre", text("genre"));
    put("publisher", text("manufacturer"));
    put("franchise", text("serie"));
    let players = g["players"].as_u64().map(|n| n.to_string());
    put("users", players.as_deref().filter(|p| *p != "0"));
    // "19??" for an unsure year: only a whole one is kept.
    put(
        "releaseyear",
        text("year").filter(|y| y.len() == 4 && y.bytes().all(|b| b.is_ascii_digit())),
    );
    Found { art, fields }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_gives_its_pictures_facts_and_credited_history() {
        let g = serde_json::json!({
            "game_name": "atetris", "url": "https://adb.arcadeitalia.net/?mame=atetris",
            "manufacturer": "Atari Games", "genre": "Puzzle / Drop", "players": 2, "year": "1988",
            "serie": "Tetris", "history": "Tetris (c) 1989 Atari Games.",
            "history_copyright_short": "(C) arcade-history.com",
            "url_image_ingame": "https://x/in.png", "url_image_flyer": "https://x/fly.png", "url_image_title": "",
            "url_video_shortplay": "https://x/v.mp4"
        });
        let f = read(&g, false);
        let kinds: Vec<ArtKind> = f.art.iter().map(|a| a.kind).collect();
        assert_eq!(
            kinds,
            [ArtKind::Snap, ArtKind::Flyer],
            "an empty URL is none; no video without the setting"
        );
        let get = |k: &str| f.fields.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("description"), Some("Tetris (c) 1989 Atari Games."));
        assert_eq!(get("description_licence"), Some("(C) arcade-history.com"));
        assert_eq!(get("publisher"), Some("Atari Games"));
        assert_eq!(get("users"), Some("2"));
        assert_eq!(get("releaseyear"), Some("1988"));
        assert_eq!(get("franchise"), Some("Tetris"));
        assert!(read(&g, true).art.iter().any(|a| a.kind == ArtKind::Video));
        let unsure = serde_json::json!({"year": "19??"});
        assert!(
            read(&unsure, false)
                .fields
                .iter()
                .all(|(k, _)| *k != "releaseyear")
        );
    }
}
