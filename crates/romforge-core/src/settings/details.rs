//! What the fullscreen mode shows of the game in focus: each detail can be
//! turned off, in Settings → Display or in the mode's Start → Details. One
//! set for every layout.

use rusqlite::{Connection, OptionalExtension, params};

use crate::Error;

/// Every detail, in the order they are shown and listed, with its name
/// in Settings and Start → Details: the logo (else the title), the
/// platform, the release date, genre, players, game modes, developer,
/// publisher, age rating, community rating, region and edition, what a
/// hack or translation was made from, play time, the overview, and the
/// fanart behind the home screen.
pub const FS_DETAILS: &[(&str, &str)] = &[
    ("logo", "Logo (else title)"),
    ("platform", "Platform"),
    ("released", "Release date"),
    ("genre", "Genre"),
    ("players", "Players"),
    ("modes", "Modes"),
    ("developer", "Developer"),
    ("publisher", "Publisher"),
    ("rating", "Age rating"),
    ("community", "Community rating"),
    ("edition", "Region and edition"),
    ("derived", "Hack or translation of"),
    ("played", "Play time"),
    ("overview", "Overview"),
    ("backdrop", "Fanart backdrop"),
];

/// The ones shown until the user picks: the rest would make the screen
/// busier than it was before they existed.
const SHOWN: &[&str] = &[
    "logo",
    "platform",
    "released",
    "genre",
    "players",
    "developer",
    "community",
    "derived",
    "played",
    "overview",
];

/// The `settings` row they are kept in: the shown ones, comma-separated
/// (empty: none shown; no row: [`SHOWN`]).
const KEY: &str = "fs_details";

/// A choice as the app lists it: the value kept and its name.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Choice {
    pub value: &'static str,
    pub label: &'static str,
}

pub(super) fn choices(of: &[(&'static str, &'static str)]) -> Vec<Choice> {
    of.iter().map(|&(value, label)| Choice { value, label }).collect()
}

/// What the app needs to list the details: each one, and those shown
/// until the user picks.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DetailsInfo {
    pub details: Vec<Choice>,
    pub shown: Vec<String>,
}

pub fn fs_details_info() -> DetailsInfo {
    DetailsInfo {
        details: choices(FS_DETAILS),
        shown: default_fs_details(),
    }
}

pub fn default_fs_details() -> Vec<String> {
    SHOWN.iter().map(|&d| d.to_owned()).collect()
}

/// `shown` as it is kept: known details only, each once, in
/// [`FS_DETAILS`]' order.
pub fn clean_fs_details<S: AsRef<str>>(shown: &[S]) -> Vec<String> {
    FS_DETAILS
        .iter()
        .filter(|(d, _)| shown.iter().any(|s| s.as_ref() == *d))
        .map(|&(d, _)| d.to_owned())
        .collect()
}

pub(super) fn load(conn: &Connection) -> Result<Vec<String>, Error> {
    let row: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [KEY], |r| r.get(0))
        .optional()?;
    Ok(match row {
        Some(v) => clean_fs_details(&v.split(',').map(str::trim).collect::<Vec<_>>()),
        None => default_fs_details(),
    })
}

/// Keep `shown` (cleaned) and return what was kept.
pub fn save_fs_details<S: AsRef<str>>(conn: &Connection, shown: &[S]) -> Result<Vec<String>, Error> {
    let shown = clean_fs_details(shown);
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![KEY, shown.join(",")],
    )?;
    Ok(shown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT)", [])
            .unwrap();
        c
    }

    #[test]
    fn unset_shows_the_defaults_and_none_stays_none() {
        let c = conn();
        assert_eq!(load(&c).unwrap(), default_fs_details());
        assert!(!default_fs_details().contains(&"backdrop".to_owned()));
        assert!(
            default_fs_details()
                .iter()
                .all(|d| FS_DETAILS.iter().any(|(k, _)| k == d))
        );
        save_fs_details::<&str>(&c, &[]).unwrap();
        assert!(load(&c).unwrap().is_empty(), "all turned off stays off");
    }

    #[test]
    fn kept_in_order_without_unknowns_or_repeats() {
        let c = conn();
        let kept = save_fs_details(&c, &["overview", "wings", "logo", "logo"]).unwrap();
        assert_eq!(kept, ["logo", "overview"]);
        assert_eq!(load(&c).unwrap(), kept);
    }
}
