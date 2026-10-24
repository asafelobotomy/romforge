//! How the fullscreen mode's bookshelf layout stands the picked
//! platform's games: one endless shelf each or a bookcase of shelves
//! holding at most [`SHELF_MAX`] boxes, in an order, All games' by
//! platform or all mixed, with its favourites on a shelf at the top and
//! those played lately under it if wanted. Kept
//! in Settings → Display, or changed in the mode's Start → Bookshelf.

use rusqlite::{Connection, OptionalExtension, params};

use super::details::{Choice, choices};
use crate::Error;

/// The most boxes a shelf holds in the bookcase (`vertical`), and with
/// the details beside the shelves rather than above them.
pub const SHELF_MAX: usize = 30;
pub const SHELF_MAX_BESIDE: usize = 15;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Bookshelf {
    /// "horizontal": each shelf goes on endlessly; "vertical": shelves of
    /// at most [`SHELF_MAX`], down to the next.
    pub direction: String,
    /// "platform": a platform's games on shelves of their own (All
    /// games); "mixed": all on the same shelves.
    pub group: String,
    /// A shelf of favourites at the top.
    pub favourites: bool,
    /// A shelf of the games played lately, under it.
    pub recent: bool,
    /// The games' order on the shelves: "title", "released" (oldest
    /// first), "newest" or "played" (longest played first).
    pub sort: String,
    /// Where the focused game's details go: "top" (above the shelves),
    /// "left" or "right" (beside them, the shelves shorter: at most
    /// [`SHELF_MAX_BESIDE`] boxes).
    pub details: String,
}

/// Each choice the bookshelf takes, with its name in Settings and Start →
/// Bookshelf; the first is the default's.
pub const SHELF_DIRECTIONS: &[(&str, &str)] = &[
    ("horizontal", "One endless shelf each"),
    (
        "vertical",
        "Shelves of 30 (15 beside the details), down to the next",
    ),
];
pub const SHELF_GROUPS: &[(&str, &str)] = &[
    ("platform", "A shelf for each platform"),
    ("mixed", "All platforms mixed"),
];
pub const SHELF_SORTS: &[(&str, &str)] = &[
    ("title", "Name"),
    ("released", "Release date, oldest first"),
    ("newest", "Release date, newest first"),
    ("played", "Most played"),
];
pub const SHELF_DETAILS: &[(&str, &str)] = &[
    ("top", "Above the shelves"),
    ("left", "On the left"),
    ("right", "On the right"),
];

/// What the app needs to show the bookshelf's choices and stand its
/// shelves: the default, the most boxes a shelf holds, and each list.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BookshelfInfo {
    pub default: Bookshelf,
    pub max: usize,
    pub max_beside: usize,
    pub directions: Vec<Choice>,
    pub groups: Vec<Choice>,
    pub sorts: Vec<Choice>,
    pub details: Vec<Choice>,
}

pub fn bookshelf_info() -> BookshelfInfo {
    BookshelfInfo {
        default: Bookshelf::default(),
        max: SHELF_MAX,
        max_beside: SHELF_MAX_BESIDE,
        directions: choices(SHELF_DIRECTIONS),
        groups: choices(SHELF_GROUPS),
        sorts: choices(SHELF_SORTS),
        details: choices(SHELF_DETAILS),
    }
}

impl Default for Bookshelf {
    fn default() -> Self {
        Bookshelf {
            direction: "horizontal".into(),
            group: "platform".into(),
            favourites: false,
            recent: false,
            sort: "title".into(),
            details: "top".into(),
        }
    }
}

impl Bookshelf {
    /// As it is kept: an unknown direction, grouping, order or place for
    /// the details is the default's.
    pub fn cleaned(&self) -> Bookshelf {
        let d = Bookshelf::default();
        let pick = |v: &str, of: &[(&str, &str)], or: String| {
            of.iter().find(|x| x.0 == v).map_or(or, |x| x.0.to_owned())
        };
        Bookshelf {
            direction: pick(&self.direction, SHELF_DIRECTIONS, d.direction),
            group: pick(&self.group, SHELF_GROUPS, d.group),
            sort: pick(&self.sort, SHELF_SORTS, d.sort),
            details: pick(&self.details, SHELF_DETAILS, d.details),
            ..self.clone()
        }
    }
}

/// The `settings` row it is kept in, as JSON.
const KEY: &str = "fs_bookshelf";

pub(super) fn load(conn: &Connection) -> Result<Bookshelf, Error> {
    let row: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [KEY], |r| r.get(0))
        .optional()?;
    Ok(row
        .and_then(|v| serde_json::from_str::<Bookshelf>(&v).ok())
        .unwrap_or_default()
        .cleaned())
}

/// Keep `b` (cleaned) and return what was kept.
pub fn save_bookshelf(conn: &Connection, b: &Bookshelf) -> Result<Bookshelf, Error> {
    let b = b.cleaned();
    let json = serde_json::to_string(&b).map_err(|e| Error::Settings(e.to_string()))?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![KEY, json],
    )?;
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_cleaned_and_unset_is_the_default() {
        let c = Connection::open_in_memory().unwrap();
        c.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT)", [])
            .unwrap();
        assert_eq!(load(&c).unwrap(), Bookshelf::default());
        let b = Bookshelf {
            direction: "vertical".into(),
            group: "diagonal".into(),
            favourites: true,
            recent: false,
            sort: "newest".into(),
            details: "left".into(),
        };
        let kept = save_bookshelf(&c, &b).unwrap();
        assert_eq!(
            (kept.direction.as_str(), kept.group.as_str()),
            ("vertical", "platform")
        );
        assert_eq!((kept.sort.as_str(), kept.details.as_str()), ("newest", "left"));
        assert_eq!(load(&c).unwrap(), kept);
    }

    #[test]
    fn the_lists_lead_with_the_default_and_name_the_limits() {
        let d = Bookshelf::default();
        let first = |of: &[(&'static str, &'static str)]| of[0].0;
        assert_eq!(first(SHELF_DIRECTIONS), d.direction);
        assert_eq!(first(SHELF_GROUPS), d.group);
        assert_eq!(first(SHELF_SORTS), d.sort);
        assert_eq!(first(SHELF_DETAILS), d.details);
        let (_, vertical) = SHELF_DIRECTIONS[1];
        assert!(vertical.contains(&format!("of {SHELF_MAX} ({SHELF_MAX_BESIDE} beside")));
    }
}
