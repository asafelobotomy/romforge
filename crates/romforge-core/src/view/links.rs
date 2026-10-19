//! Links for a game, and metadata values as shown.

use std::collections::BTreeMap;

use super::Link;
use super::sort::search_title;
use crate::fetch;

/// Links for a game: its own pages where Wikidata gave an ID (in
/// `fields`), plain searches otherwise (house rule 10, tier 1).
/// A metadata value as shown. libretro writes `???` for what it doesn't
/// know: alone it means nothing is known; next to a name ("Nintendo ???")
/// the name is a guess, and says so. Wikidata replaces such values when it
/// matches the game.
pub fn shown_value(v: &str) -> Option<String> {
    if !v.contains("???") {
        return Some(v.to_owned()).filter(|v| !v.trim().is_empty());
    }
    let rest = v.replace("???", "");
    let rest = rest.trim().trim_matches(',').trim();
    (!rest.is_empty()).then(|| format!("{rest} (unconfirmed)"))
}

/// Where to look for a translation or hack of a game (searches, opened in
/// the browser; romforge fetches no patch itself, milestone 19).
pub fn patch_links(title: &str) -> Vec<Link> {
    let q = fetch::encode_segment(&search_title(title));
    [
        (
            "Translations (romhacking.net)",
            format!("https://www.romhacking.net/?page=translations&title={q}"),
        ),
        (
            "Hacks (romhacking.net)",
            format!("https://www.romhacking.net/?page=hacks&title={q}"),
        ),
        ("Romhack Plaza", format!("https://romhackplaza.org/?s={q}")),
    ]
    .into_iter()
    .map(|(label, url)| Link {
        label: label.into(),
        url,
        direct: false,
    })
    .collect()
}

pub fn links(title: &str, system_name: Option<&str>, fields: &BTreeMap<String, String>) -> Vec<Link> {
    let q = search_title(title);
    let enc = |s: &str| fetch::encode_segment(s);
    let wiki_q = match system_name {
        Some(_) => format!("{q} video game"),
        None => q.clone(),
    };
    let first = |k: &str| {
        fields
            .get(k)
            .and_then(|v| v.split(", ").next())
            .map(str::to_owned)
    };
    let link = |label: &str, direct: Option<String>, search: String| match direct {
        Some(url) => Link {
            label: label.into(),
            url,
            direct: true,
        },
        None => Link {
            label: label.into(),
            url: search,
            direct: false,
        },
    };
    let mut out = vec![
        link(
            "MobyGames",
            first("mobygames_id")
                .map(|id| format!("https://www.mobygames.com/game/{}/", enc(&id)))
                .or_else(|| {
                    first("mobygames_slug").map(|s| format!("https://www.mobygames.com/game/{}", enc(&s)))
                }),
            format!("https://www.mobygames.com/search/?q={}", enc(&q)),
        ),
        link(
            "RetroAchievements",
            first("retroachievements_id")
                .map(|id| format!("https://retroachievements.org/game/{}", enc(&id))),
            format!("https://retroachievements.org/searchresults.php?s={}", enc(&q)),
        ),
        link(
            "Wikipedia",
            first("wikipedia")
                .map(|a| format!("https://en.wikipedia.org/wiki/{}", enc(&a.replace(' ', "_")))),
            format!("https://en.wikipedia.org/w/index.php?search={}", enc(&wiki_q)),
        ),
        link(
            "IGDB",
            first("igdb_slug").map(|s| format!("https://www.igdb.com/games/{}", enc(&s))),
            format!("https://www.igdb.com/search?q={}", enc(&q)),
        ),
        link(
            "HowLongToBeat",
            first("hltb_id").map(|id| format!("https://howlongtobeat.com/game/{}", enc(&id))),
            format!("https://howlongtobeat.com/?q={}", enc(&q)),
        ),
    ];
    if let Some(qid) = first("wikidata") {
        out.push(Link {
            label: "Wikidata".into(),
            url: format!("https://www.wikidata.org/wiki/{}", enc(&qid)),
            direct: true,
        });
    }
    // A video of the game (LaunchBox gives one), https only.
    if let Some(v) = fields.get("video").filter(|v| v.starts_with("https://")) {
        out.push(Link {
            label: "Video".into(),
            url: v.clone(),
            direct: true,
        });
    }
    out
}
