//! Finishing a view: sorting, grouping, and counting games per platform.

use std::collections::BTreeMap;

use rusqlite::Connection;

use super::pictures::{pictures, release_years};
use super::sort::sort_key;
use super::versions::group_versions;
use super::{GameRow, LibraryView, Platform};
use crate::{Error, systems};

/// Sort, group versions, count games per platform.
pub(super) fn finish(conn: &Connection, mut rows: Vec<GameRow>, listing: bool) -> Result<LibraryView, Error> {
    let shown = |r: &GameRow| sort_key(r.label.as_deref().unwrap_or(&r.title));
    rows.sort_by(|a, b| shown(a).cmp(&shown(b)).then_with(|| a.system.cmp(&b.system)));
    group_versions(conn, &mut rows, listing)?;
    release_years(conn, &mut rows)?;
    pictures(conn, &mut rows)?;
    if listing {
        // Originals added for missing games go in their place.
        rows.sort_by(|a, b| shown(a).cmp(&shown(b)).then_with(|| a.system.cmp(&b.system)));
    }
    // Games, not versions; whatever isn't matched yet counts under
    // Unidentified whatever its platform.
    for r in &mut rows {
        r.unmatched = r.status.unmatched() || r.system_id.is_none();
    }
    let mut counts: BTreeMap<Option<i64>, usize> = BTreeMap::new();
    for r in rows.iter().filter(|r| r.main) {
        *counts
            .entry(if r.unmatched { None } else { r.system_id })
            .or_default() += 1;
    }
    let mut platforms: Vec<Platform> = counts
        .into_iter()
        .map(|(id, games)| {
            let sys = id.and_then(systems::by_id);
            Platform {
                system: sys.map(|s| s.key.to_owned()),
                short: sys.map_or("?".to_owned(), |s| short_name(s.key)),
                name: sys.map_or("Unidentified".to_owned(), |s| s.name.to_owned()),
                manufacturer: sys.map(|s| s.manufacturer.to_owned()),
                games,
            }
        })
        .collect();
    platforms.sort_by(|a, b| (a.system.is_none(), &a.name).cmp(&(b.system.is_none(), &b.name)));
    Ok(LibraryView { platforms, rows })
}

/// An icon label from a system key: short keys as they are ("SNES",
/// "Lynx"), longer ones as their capitals and digits ("Game Boy Advance" ->
/// "GBA", "PlayStation 2" -> "PS2", "Commodore 64" -> "C64").
pub fn short_name(key: &str) -> String {
    if key.chars().count() <= 5 {
        return key.to_owned();
    }
    let s: String = key
        .chars()
        .filter(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        .collect();
    if s.chars().count() >= 2 {
        s.chars().take(5).collect()
    } else {
        key.chars()
            .filter(|c| c.is_alphanumeric())
            .take(3)
            .collect::<String>()
            .to_uppercase()
    }
}
