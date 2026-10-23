//! SteamGridDB (tier 3): a game by its title, only when exactly one game
//! has it (it knows no platforms): hero, logo and icon pictures, the
//! largest first.

use super::{Bases, Found, Job, json};
use crate::enrich::{ArtFound, ArtKind};
use crate::fetch::{self, Request};
use crate::settings::Keys;
use crate::{Error, wikidata};

pub(super) fn lookup(keys: &Keys, bases: &Bases, job: &Job) -> Result<Found, Error> {
    let Some(key) = &keys.steamgriddb_key else {
        return Ok(Found::default());
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
        return Ok(Found::default());
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
    let [id] = named[..] else {
        return Ok(Found::default());
    };
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
        let mut items: Vec<&serde_json::Value> = v["data"].as_array().into_iter().flatten().collect();
        // Plain styles before blurred or material ones, then the largest
        // first: the best of a kind is taken.
        items.sort_by_key(|x| {
            let styled = matches!(x["style"].as_str(), Some("blurred" | "material"));
            let area = x["width"].as_u64().unwrap_or(0) * x["height"].as_u64().unwrap_or(0);
            (styled, std::cmp::Reverse(area))
        });
        for x in items {
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
    Ok(Found {
        art: out,
        fields: Vec::new(),
    })
}
