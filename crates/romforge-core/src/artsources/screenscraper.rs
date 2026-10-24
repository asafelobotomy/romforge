//! ScreenScraper (tier 2): a game by its ROM's hashes, the strongest match.
//! One `jeuInfos` request gives its pictures and its text: the synopsis in
//! the chosen language (else English), age ratings, players, release
//! dates by region, genres, developer and publisher.

use serde_json::Value;

use super::{Bases, Found, Job, json, without_credentials};
use crate::Error;
use crate::enrich::{ArtFound, ArtKind};
use crate::fetch;
use crate::settings::Keys;

/// ScreenScraper's picture types, as romforge's kinds.
fn kind(t: &str) -> Option<ArtKind> {
    Some(match t {
        "box-2D" => ArtKind::Boxart,
        "box-3D" => ArtKind::Box3d,
        "box-2D-side" => ArtKind::Spine,
        "box-2D-back" => ArtKind::Boxback,
        "box-texture" => ArtKind::Boxfull,
        "support-2D" | "support-texture" | "support-scan" => ArtKind::Media,
        "screenmarquee" | "marquee" | "screenmarqueesmall" => ArtKind::Marquee,
        "flyer" => ArtKind::Flyer,
        // The smaller, re-encoded clip before the original.
        "video-normalized" | "video" => ArtKind::Video,
        "wheel" | "wheel-hd" => ArtKind::Logo,
        "fanart" => ArtKind::Hero,
        "sstitle" => ArtKind::Title,
        "ss" => ArtKind::Snap,
        _ => return None,
    })
}

pub(super) fn lookup(keys: &Keys, bases: &Bases, job: &Job) -> Result<Found, Error> {
    let (Some(system_id), Some(rom)) = (job.system.screenscraper, &job.rom) else {
        return Ok(Found::default());
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
        return Ok(Found::default());
    };
    Ok(read(&json(&body)?["response"]["jeu"], job))
}

/// A `jeu` object's pictures (the game's own region first, then
/// Settings', [`crate::enrich::regions::screenscraper`]) and fields.
pub(super) fn read(game: &Value, job: &Job) -> Found {
    let order = crate::enrich::regions::screenscraper(job.title, job.prefs);
    let rank = |r: &str| order.iter().position(|x| *x == r).unwrap_or(order.len());
    let mut art: Vec<(usize, ArtFound)> = Vec::new();
    let mut manual: Option<(usize, String)> = None;
    for m in game["medias"].as_array().into_iter().flatten() {
        let t = m["type"].as_str().unwrap_or_default();
        if t == "manuel"
            && let Some(url) = m["url"].as_str()
        {
            let r = rank(m["region"].as_str().unwrap_or("ss"));
            if manual.as_ref().is_none_or(|(best, _)| r < *best) {
                manual = Some((r, without_credentials(url)));
            }
            continue;
        }
        let Some(kind) = kind(t) else {
            continue;
        };
        if kind == ArtKind::Video && !job.video {
            continue;
        }
        let Some(url) = m["url"].as_str() else { continue };
        let region = m["region"].as_str().unwrap_or("ss");
        // Its media URLs carry the credentials; they are put back only to
        // download ([`super::download_url`]).
        let url = without_credentials(url);
        let sep = if url.contains('?') { '&' } else { '?' };
        // A normalized clip before the original, whatever the region.
        let video_rank = usize::from(kind == ArtKind::Video && t == "video") * 1000;
        art.push((
            rank(region) + video_rank,
            ArtFound {
                kind,
                // ScreenScraper scales its media on request.
                thumb: Some(format!("{url}{sep}maxwidth=480")),
                url,
                label: Some(region.to_uppercase()),
            },
        ));
    }
    art.sort_by_key(|(r, _)| *r);
    let mut fields = fields(game, job.language);
    // Its manual, offered in the Manual tab (the credentials put back only
    // to download it).
    if let Some((_, url)) = manual {
        fields.push(("manual_screenscraper", url));
    }
    Found {
        art: art.into_iter().map(|(_, f)| f).collect(),
        fields,
    }
}

/// A list of `{langue, text}` in `lang`, else English, else the first.
fn in_language<'a>(list: &'a Value, lang: &str) -> Option<&'a str> {
    let items = list.as_array()?;
    let text = |l: &str| {
        items
            .iter()
            .find(|i| i["langue"].as_str() == Some(l))
            .and_then(|i| i["text"].as_str())
    };
    text(lang)
        .or_else(|| text("en"))
        .or_else(|| items.first().and_then(|i| i["text"].as_str()))
}

fn fields(game: &Value, lang: &str) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::new();
    let mut put = |k: &'static str, v: Option<&str>| {
        if let Some(v) = v.map(str::trim).filter(|v| !v.is_empty()) {
            out.push((k, v.to_owned()));
        }
    };
    put("description", in_language(&game["synopsis"], lang));
    put("developer", game["developpeur"]["text"].as_str());
    put("publisher", game["editeur"]["text"].as_str());
    put("users", game["joueurs"]["text"].as_str());
    for c in game["classifications"].as_array().into_iter().flatten() {
        let field = match c["type"].as_str().unwrap_or_default() {
            "ESRB" => "esrb",
            "PEGI" => "pegi",
            "CERO" => "cero",
            "USK" => "usk",
            _ => continue,
        };
        put(field, c["text"].as_str());
    }
    let genres: Vec<&str> = game["genres"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| in_language(&g["noms"], "en"))
        .collect();
    put("genre", Some(&genres.join(", ")));
    let mut dates: Vec<(String, &str)> = game["dates"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| {
            Some((
                d["text"].as_str()?.to_owned(),
                d["region"].as_str().unwrap_or("ss"),
            ))
        })
        .collect();
    dates.sort();
    dates.dedup();
    out.extend(crate::enrich::release_fields(dates.iter().map(
        |(date, region)| (date.as_str(), crate::enrich::regions::name_of_code(region)),
    )));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_gives_its_pictures_in_region_order_and_its_text() {
        let game = serde_json::json!({
            "synopsis": [{"langue": "en", "text": "In English."}, {"langue": "fr", "text": "En français."}],
            "developpeur": {"text": "Nintendo R&D1"},
            "joueurs": {"text": "1-2"},
            "classifications": [{"type": "PEGI", "text": "3"}, {"type": "SS", "text": "x"}],
            "genres": [{"noms": [{"langue": "de", "text": "Plattform"}, {"langue": "en", "text": "Platform"}]}],
            "dates": [{"region": "us", "text": "1994-04-18"}, {"region": "jp", "text": "1994-03-19"}],
            "medias": [
                {"type": "box-2D", "region": "us", "url": "https://x/us.png"},
                {"type": "box-2D", "region": "eu", "url": "https://x/eu.png"},
                {"type": "box-texture", "region": "eu", "url": "https://x/full.png"},
                {"type": "support-2D", "region": "eu", "url": "https://x/cart.png"},
                {"type": "video", "region": "eu", "url": "https://x/v.mp4"},
                {"type": "flyer", "region": "eu", "url": "https://x/fly.png"},
                {"type": "manuel", "region": "us", "url": "https://x/m.pdf?ssid=me&sspassword=secret"}
            ]
        });
        let system = crate::systems::by_key("SNES").unwrap();
        let prefs = vec!["Europe".to_owned(), "USA".to_owned()];
        let job = Job {
            system,
            title: "Game (Europe)",
            rom: None,
            prefs: &prefs,
            language: "fr",
            year: None,
            serial: None,
            video: false,
        };
        let found = read(&game, &job);
        let kinds: Vec<(ArtKind, &str)> = found
            .art
            .iter()
            .map(|a| (a.kind, a.label.as_deref().unwrap_or("")))
            .collect();
        assert_eq!(kinds[0], (ArtKind::Boxart, "EU"), "its own region first");
        assert!(kinds.contains(&(ArtKind::Boxfull, "EU")) && kinds.contains(&(ArtKind::Media, "EU")));
        assert_eq!(found.art.len(), 5, "no video without the setting");
        assert!(kinds.contains(&(ArtKind::Flyer, "EU")));
        let get = |k: &str| {
            found
                .fields
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("description"), Some("En français."));
        assert_eq!(get("developer"), Some("Nintendo R&D1"));
        assert_eq!(get("users"), Some("1-2"));
        assert_eq!(get("pegi"), Some("3"));
        assert_eq!(get("genre"), Some("Platform"));
        assert_eq!(get("releaseyear"), Some("1994"));
        assert_eq!(get("releases"), Some("1994-03-19 Japan; 1994-04-18 USA"));
        let manual = get("manual_screenscraper").unwrap();
        assert!(
            manual.starts_with("https://x/m.pdf") && !manual.contains("secret"),
            "{manual}"
        );
        let with_video = Job { video: true, ..job };
        assert!(
            read(&game, &with_video)
                .art
                .iter()
                .any(|a| a.kind == ArtKind::Video)
        );
    }
}
