//! Where a game's manual can be found (milestone 26), by its No-Intro
//! name, all on archive.org (which welcomes programs: an open search API,
//! plain download links):
//! 1. retrokit-manuals: scans for 46 systems in one zip per system, each
//!    named `<parent title without region> (<languages>).pdf` (the DAT's
//!    parent: "Castlevania - Bloodlines (USA)" is under "Castlevania - The
//!    New Generation") and fetched alone from inside the zip. The listing
//!    of a system's zip is kept a month.
//! 2. Kirkland's Manual Labor (also videogamemanual.com): one item per
//!    manual, titled with the exact No-Intro name, found with archive.org's
//!    search API.
//!
//! Vimm's Lair (by hash, through Hasheous) is the third way. Offers are only
//! offered: a manual is downloaded when the user picks one.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rusqlite::Connection;

use crate::Error;

const RETROKIT: &str = "https://archive.org/download/retrokit-manuals/";
const SEARCH: &str = "https://archive.org/advancedsearch.php";
const METADATA: &str = "https://archive.org/metadata/";
const DOWNLOAD: &str = "https://archive.org/download/";
const LISTING_AGE: Duration = Duration::from_secs(30 * 24 * 3600);

/// A manual found for a game, to download if the user picks it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ManualOffer {
    /// "retrokit-manuals", "Kirkland's Manual Labor".
    pub source: String,
    /// The file's name there.
    pub name: String,
    /// Its languages ("en", "en-gb,de,fr"), when the name says.
    pub langs: String,
    pub url: String,
    pub size: Option<u64>,
}

/// A No-Intro name without its parenthesised tags: "Super Mario Kart (USA)"
/// is "Super Mario Kart".
fn base(title: &str) -> &str {
    title.split(" (").next().unwrap_or(title).trim()
}

/// A name as compared: lower case, letters and digits only.
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The languages a release's regions read, most likely first.
fn region_langs(title: &str) -> Vec<&'static str> {
    let tags = title.split_once(" (").map_or("", |(_, t)| t);
    let mut out = Vec::new();
    for (region, langs) in [
        ("USA", &["en"][..]),
        ("Canada", &["en", "fr"]),
        ("Australia", &["en-gb", "en"]),
        ("UK", &["en-gb", "en"]),
        ("Europe", &["en-gb", "en", "de", "fr"]),
        ("Japan", &["ja"]),
        ("Germany", &["de"]),
        ("France", &["fr"]),
        ("Spain", &["es"]),
        ("Italy", &["it"]),
        ("Netherlands", &["nl"]),
        ("Sweden", &["sv"]),
        ("Korea", &["ko"]),
    ] {
        if tags.contains(region) {
            for l in langs {
                if !out.contains(l) {
                    out.push(*l);
                }
            }
        }
    }
    out
}

/// The PDFs (name, size) in a retrokit zip's listing page.
fn parse_listing(html: &str) -> Vec<(String, Option<u64>)> {
    html.split("<tr>")
        .filter_map(|row| {
            let name = row.split_once(".pdf\">")?.1.split_once("</a>")?.0;
            let size = row
                .split_once("id=\"size\">")
                .and_then(|(_, s)| s.split('<').next()?.trim().parse().ok());
            Some((html_unescape(name), size))
        })
        .filter(|(n, _)| n.ends_with(".pdf"))
        .collect()
}

fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// A retrokit listing's manuals for `title` (or its DAT parent `parent`),
/// its region's languages first.
fn retrokit_matches(
    folder: &str,
    listing: &[(String, Option<u64>)],
    title: &str,
    parent: Option<&str>,
) -> Vec<ManualOffer> {
    let want: Vec<String> = std::iter::once(title)
        .chain(parent)
        .map(|t| norm(base(t)))
        .collect();
    let mut out: Vec<ManualOffer> = listing
        .iter()
        .filter_map(|(name, size)| {
            let stem = name.strip_suffix(".pdf")?;
            let (t, langs) = stem
                .rsplit_once(" (")
                .map_or((stem, ""), |(t, l)| (t, l.trim_end_matches(')')));
            want.contains(&norm(t)).then(|| ManualOffer {
                source: "retrokit-manuals".into(),
                name: name.clone(),
                langs: langs.into(),
                url: format!(
                    "{RETROKIT}{folder}/{folder}-compressed.zip/{}",
                    crate::fetch::percent_encode(name)
                ),
                size: *size,
            })
        })
        .collect();
    let prefs = region_langs(title);
    let rank = |o: &ManualOffer| {
        let langs: Vec<&str> = o.langs.split(',').collect();
        prefs
            .iter()
            .position(|p| langs.contains(p))
            .unwrap_or(prefs.len() + usize::from(!langs.iter().any(|l| l.starts_with("en"))))
    };
    out.sort_by_key(rank);
    out
}

/// A system's retrokit listing, from the cache while it's fresh.
fn listing(cache: &Path, folder: &str) -> Result<Vec<(String, Option<u64>)>, Error> {
    let kept = cache.join("manuals").join(format!("retrokit-{folder}.html"));
    let fresh = std::fs::metadata(&kept)
        .and_then(|m| m.modified())
        .is_ok_and(|t| SystemTime::now().duration_since(t).unwrap_or_default() < LISTING_AGE);
    let html = if fresh {
        std::fs::read_to_string(&kept)?
    } else {
        let body = crate::fetch::get_small_optional(&format!("{RETROKIT}{folder}/{folder}-compressed.zip/"))?
            .unwrap_or_default();
        crate::atomic::write(&kept, &body)?;
        String::from_utf8_lossy(&body).into_owned()
    };
    Ok(parse_listing(&html))
}

/// The identifiers of archive.org items in a search answer.
fn search_ids(json: &[u8]) -> Vec<String> {
    let v: serde_json::Value = serde_json::from_slice(json).unwrap_or_default();
    v.pointer("/response/docs")
        .and_then(|d| d.as_array())
        .into_iter()
        .flatten()
        .filter_map(|d| d.get("identifier")?.as_str().map(str::to_owned))
        .collect()
}

/// An item's original PDF (name, size) from its metadata files.
fn item_pdf(json: &[u8]) -> Option<(String, Option<u64>)> {
    let v: serde_json::Value = serde_json::from_slice(json).ok()?;
    v.get("result")?.as_array()?.iter().find_map(|f| {
        let name = f.get("name")?.as_str()?;
        (name.to_ascii_lowercase().ends_with(".pdf") && f.get("source")?.as_str()? == "original").then(|| {
            (
                name.to_owned(),
                f.get("size").and_then(|s| s.as_str()?.parse().ok()),
            )
        })
    })
}

/// A No-Intro name cut after its region: "Donkey Kong Country (USA) (Rev
/// 2)" is "Donkey Kong Country (USA)", as Kirkland titles his scans.
fn with_region(title: &str) -> Option<&str> {
    let (_, tags) = title.split_once(" (")?;
    Some(&title[..title.len() - tags.len() + tags.find(')')? + 1])
}

/// Kirkland's scan of `title` (a No-Intro name with its region).
fn kirkland(title: &str) -> Result<Vec<ManualOffer>, Error> {
    let Some(title) = with_region(title).filter(|t| !t.contains('"')) else {
        return Ok(Vec::new());
    };
    let q = format!("kirklands-manual-labor AND title:(\"{title}\")");
    let url = format!(
        "{SEARCH}?q={}&fl[]=identifier&rows=3&output=json",
        crate::fetch::percent_encode(&q)
    );
    let mut out = Vec::new();
    for id in search_ids(&crate::fetch::get_small_optional(&url)?.unwrap_or_default()) {
        let files = crate::fetch::get_small_optional(&format!("{METADATA}{id}/files"))?.unwrap_or_default();
        if let Some((name, size)) = item_pdf(&files) {
            out.push(ManualOffer {
                source: "Kirkland's Manual Labor".into(),
                url: format!("{DOWNLOAD}{id}/{}", crate::fetch::percent_encode(&name)),
                name,
                langs: String::new(),
                size,
            });
        }
    }
    Ok(out)
}

/// Every manual found for the game (`parent`: its DAT parent, if a
/// clone), best first: retrokit's in the game's language, then Kirkland's,
/// then retrokit's other languages.
pub fn find_manuals(
    cache: &Path,
    system: &str,
    title: &str,
    parent: Option<&str>,
) -> Result<Vec<ManualOffer>, Error> {
    let folders = crate::systems::by_key(system).map_or(&[][..], |s| s.manuals);
    let mut rk = Vec::new();
    for folder in folders {
        rk.extend(retrokit_matches(folder, &listing(cache, folder)?, title, parent));
    }
    let langs = region_langs(title);
    let (mut out, rest): (Vec<_>, Vec<_>) = rk
        .into_iter()
        .partition(|o| o.langs.split(',').any(|l| langs.contains(&l)));
    out.extend(kirkland(title)?);
    out.extend(rest);
    Ok(out)
}

/// Whether `url` is from a place romforge offers manuals from: archive.org,
/// the Arcade Database, ScreenScraper.
fn known_source(url: &str) -> bool {
    let host = crate::fetch::host_of(url);
    let name = host.rsplit_once(':').map_or(host, |(h, _)| h);
    if crate::fetch::has_userinfo(url) {
        return false;
    }
    url.starts_with(DOWNLOAD)
        || (url.starts_with("https://")
            && (name == "adb.arcadeitalia.net"
                || name == "screenscraper.fr"
                || name.ends_with(".screenscraper.fr")))
}

/// Download an offer (the user picked it) and keep it as the manual;
/// `keys`: ScreenScraper's are put back into its address only to download.
#[allow(clippy::too_many_arguments)]
pub fn download_offer(
    conn: &Connection,
    media: &Path,
    staging: &Path,
    system: &str,
    game: &str,
    offer: &ManualOffer,
    keys: &crate::settings::Keys,
) -> Result<PathBuf, Error> {
    if !known_source(&offer.url) {
        return Err(Error::Manual(format!(
            "not a manual romforge knows: {}",
            offer.url
        )));
    }
    let get = crate::artsources::download_url(
        crate::artsources::Provider::ScreenScraper.name(),
        &offer.url,
        keys,
        &crate::artsources::Bases::default(),
    );
    let req = crate::fetch::Request {
        max_body: Some(512 << 20),
        ..crate::fetch::Request::get(&get)
    };
    let bytes = crate::fetch::request(&req)?.unwrap_or_default();
    if !super::is_pdf(&bytes) {
        return Err(Error::Manual(format!("{} didn't send a PDF", offer.source)));
    }
    std::fs::create_dir_all(staging)?;
    let tmp = staging.join(format!("manual-{}.pdf", std::process::id()));
    crate::atomic::write(&tmp, &bytes)?;
    let kept = super::add_manual(
        conn,
        media,
        system,
        game,
        &tmp,
        &format!("{}, downloaded by you", offer.source),
    );
    let _ = std::fs::remove_file(&tmp);
    kept
}

#[cfg(test)]
mod tests;
