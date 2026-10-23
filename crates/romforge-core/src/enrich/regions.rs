//! Which region's picture to prefer: the game's own (from its DAT name),
//! then the regions in Settings, then the rest. Used for ScreenScraper's
//! region codes and for libretro's other versions of a missing thumbnail.

use rusqlite::{Connection, params};

use crate::Error;

/// No-Intro's region names and ScreenScraper's codes for them.
const CODES: &[(&str, &str)] = &[
    ("World", "wor"),
    ("USA", "us"),
    ("Europe", "eu"),
    ("Japan", "jp"),
    ("Korea", "kr"),
    ("Brazil", "br"),
    ("Australia", "au"),
    ("Canada", "ca"),
    ("China", "cn"),
    ("Taiwan", "tw"),
    ("Asia", "asi"),
    ("France", "fr"),
    ("Germany", "de"),
    ("Spain", "sp"),
    ("Italy", "it"),
    ("Netherlands", "nl"),
    ("Sweden", "se"),
    ("United Kingdom", "uk"),
    ("UK", "uk"),
];

/// A region's name for one of ScreenScraper's codes (the code itself
/// when it isn't one of [`CODES`]'s).
pub fn name_of_code(code: &str) -> &str {
    match code {
        "wor" => "World",
        "ss" => "",
        c => CODES.iter().find(|(_, x)| *x == c).map_or(c, |(r, _)| r),
    }
}

/// The regions a DAT name gives (`Game (USA, Europe) (Rev 1)`: USA,
/// Europe): the first group in brackets whose parts are all regions.
pub fn of_name(name: &str) -> Vec<String> {
    name.split('(')
        .skip(1)
        .filter_map(|g| g.split_once(')').map(|(inner, _)| inner))
        .map(|inner| inner.split(',').map(|p| p.trim().to_owned()).collect::<Vec<_>>())
        .find(|parts| !parts.is_empty() && parts.iter().all(|p| CODES.iter().any(|(r, _)| r == p)))
        .unwrap_or_default()
}

/// How good `name`'s regions are for `prefs` (lower is better): the best
/// place of any of its regions, a region-free name after every preferred
/// one.
fn place(name: &str, prefs: &[String]) -> usize {
    let regions = of_name(name);
    regions
        .iter()
        .filter_map(|r| prefs.iter().position(|p| p.eq_ignore_ascii_case(r)))
        .min()
        .unwrap_or(prefs.len() + usize::from(!regions.is_empty()))
}

/// ScreenScraper's region codes, best first: `game`'s own regions, then
/// `prefs` (Settings), then the world release, ScreenScraper's own
/// pictures, and every other region.
pub fn screenscraper(game: &str, prefs: &[String]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    let names = of_name(game).into_iter().chain(prefs.iter().cloned());
    for n in names {
        if let Some((_, c)) = CODES.iter().find(|(r, _)| r.eq_ignore_ascii_case(&n)) {
            out.push(c);
        }
    }
    out.extend(["wor", "ss"]);
    out.extend(CODES.iter().map(|(_, c)| *c));
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(*c));
    out
}

/// The other versions of `game` in its system's DATs (`Game (Europe)` for
/// `Game (USA)`), best region for `prefs` first, at most `limit`: whose
/// thumbnails to try when its own is missing.
pub fn other_versions(
    conn: &Connection,
    system_id: i64,
    game: &str,
    prefs: &[String],
    limit: usize,
) -> Result<Vec<String>, Error> {
    let Some((base, _)) = game.split_once(" (") else {
        return Ok(Vec::new());
    };
    let prefix = format!("{base} (");
    let mut names: Vec<String> = conn
        .prepare_cached(
            "SELECT DISTINCT g.name FROM games g JOIN dats d ON d.dat_id = g.dat_id
             WHERE d.system_id = ?1 AND substr(g.name, 1, length(?2)) = ?2 AND g.name != ?3",
        )?
        .query_map(params![system_id, prefix, game], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    names.sort_by_key(|n| (place(n, prefs), n.len(), n.clone()));
    names.truncate(limit);
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_come_from_the_name_and_order_by_preference() {
        assert_eq!(of_name("Game (USA, Europe) (Rev 1)"), ["USA", "Europe"]);
        assert!(of_name("Game (Proto) (Beta)").is_empty());
        let prefs = vec!["Europe".to_owned(), "USA".to_owned()];
        let ss = screenscraper("Game (Japan)", &prefs);
        assert_eq!(
            &ss[..5],
            ["jp", "eu", "us", "wor", "ss"],
            "its own, Settings', then the world"
        );
        assert!(ss.contains(&"kr") && ss.iter().filter(|c| **c == "uk").count() == 1);
        let mut names = vec![
            "G (Japan)".to_owned(),
            "G (USA)".to_owned(),
            "G (Beta)".to_owned(),
            "G (Europe)".to_owned(),
        ];
        names.sort_by_key(|n| (place(n, &prefs), n.len(), n.clone()));
        assert_eq!(names, ["G (Europe)", "G (USA)", "G (Beta)", "G (Japan)"]);
    }
}
