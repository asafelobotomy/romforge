//! ScreenScraper credentials kept out of stored and shown URLs, and IGDB
//! tokens kept for their lifetime.

use super::{Bases, Keys, Provider, fetch};

/// ScreenScraper's credential parameters: never kept, shown or logged.
const SCREENSCRAPER_SECRETS: &[&str] = &["devid", "devpassword", "ssid", "sspassword"];

/// `url` without ScreenScraper's credential parameters (its media URLs
/// carry them), as it is kept and shown.
pub fn without_credentials(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else {
        return url.to_owned();
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|kv| !SCREENSCRAPER_SECRETS.contains(&kv.split('=').next().unwrap_or(kv)))
        .collect();
    if kept.is_empty() {
        base.to_owned()
    } else {
        format!("{base}?{}", kept.join("&"))
    }
}

/// The URL to download a picture `source` gave: ScreenScraper's media
/// get the owner's credentials back, and only on ScreenScraper's own hosts
/// (`bases.screenscraper`'s, or `*.screenscraper.fr`).
pub fn download_url(source: &str, url: &str, keys: &Keys, bases: &Bases) -> String {
    let url = without_credentials(url);
    let host = fetch::host_of(&url);
    let name = host.rsplit_once(':').map_or(host, |(h, _)| h);
    let ours = !fetch::has_userinfo(&url)
        && (name == "screenscraper.fr"
            || name.ends_with(".screenscraper.fr")
            || host == fetch::host_of(&bases.screenscraper));
    if source != Provider::ScreenScraper.name() || !ours {
        return url;
    }
    let e = fetch::encode_segment;
    let mut out = url;
    for (k, v) in [
        ("devid", &keys.screenscraper_dev_id),
        ("devpassword", &keys.screenscraper_dev_password),
        ("ssid", &keys.screenscraper_user),
        ("sspassword", &keys.screenscraper_password),
    ] {
        if let Some(v) = v {
            out.push(if out.contains('?') { '&' } else { '?' });
            out.push_str(&format!("{k}={}", e(v)));
        }
    }
    out
}

/// IGDB tokens kept for the process, by (token URL, client id, secret's
/// hash), until shortly before they expire: the art picker opens a new
/// [`Session`] each time.
pub(super) fn cached_igdb_token(key: &str, new: Option<(String, u64)>) -> Option<String> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static TOKENS: Mutex<Option<HashMap<String, (String, Instant)>>> = Mutex::new(None);
    let mut tokens = TOKENS.lock().unwrap_or_else(|e| e.into_inner());
    let tokens = tokens.get_or_insert_with(HashMap::new);
    if let Some((token, secs)) = new {
        // A minute early, so a token never runs out mid-run.
        let until = Instant::now() + Duration::from_secs(secs.saturating_sub(60));
        tokens.insert(key.to_owned(), (token.clone(), until));
        return Some(token);
    }
    match tokens.get(key) {
        Some((t, until)) if Instant::now() < *until => Some(t.clone()),
        _ => None,
    }
}
