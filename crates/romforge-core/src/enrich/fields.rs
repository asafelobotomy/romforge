//! Which source's value a game's field keeps (house rule 10): every writer
//! of `game_meta` goes through [`put_field`].
//!
//! Best first: libretro, Wikidata (and Wikipedia's descriptions),
//! ScreenScraper, IGDB, then the downloaded databases (OpenVGDB, then
//! LaunchBox). A value replaces one that is missing or junk (`???`), one
//! from a later source, or its own source's. A value stored before sources
//! were recorded (`NULL`: libretro's or Wikidata's) gives way only to those
//! two.

use rusqlite::{Connection, OptionalExtension, params};

use crate::Error;

pub const LIBRETRO: &str = "libretro";
pub const WIKIDATA: &str = "wikidata";
pub const WIKIPEDIA: &str = "wikipedia";
pub const SCREENSCRAPER: &str = "screenscraper";
pub const IGDB: &str = "igdb";

/// A source's place in the order; unknown sources come last.
pub fn rank(source: &str) -> u8 {
    match source {
        LIBRETRO => 1,
        WIKIDATA | WIKIPEDIA => 2,
        SCREENSCRAPER => 3,
        IGDB => 4,
        "openvgdb" => 5,
        "launchbox" => 6,
        _ => 7,
    }
}

/// Nothing worth keeping.
pub fn junk(v: &str) -> bool {
    v.trim().is_empty() || v.contains("???")
}

/// Whether `source`'s value replaces what is stored (`had`: value, source).
pub fn replaces(source: &str, had: Option<&(String, Option<String>)>) -> bool {
    match had {
        None => true,
        Some((value, _)) if junk(value) => true,
        Some((_, None)) => rank(source) <= rank(WIKIDATA),
        Some((_, Some(old))) => old == source || rank(source) < rank(old),
    }
}

/// Store `value` as `field` of a game from `source`, if it wins (see the
/// module); `fill_only`: only where there is nothing (or junk). Returns
/// whether it was written.
pub fn put_field(
    conn: &Connection,
    system_id: i64,
    title: &str,
    field: &str,
    value: &str,
    source: &str,
    fill_only: bool,
) -> Result<bool, Error> {
    if junk(value) {
        return Ok(false);
    }
    let had: Option<(String, Option<String>)> = conn
        .prepare_cached(
            "SELECT value, source FROM game_meta WHERE system_id = ?1 AND title = ?2 AND field = ?3",
        )?
        .query_row(params![system_id, title, field], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let wins = match &had {
        Some((v, _)) if fill_only => junk(v),
        _ => replaces(source, had.as_ref()),
    };
    if !wins
        || had
            .as_ref()
            .is_some_and(|(v, s)| v == value && s.as_deref() == Some(source))
    {
        return Ok(false);
    }
    conn.prepare_cached(
        "INSERT INTO game_meta (system_id, title, field, value, source) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(system_id, title, field) DO UPDATE SET value = excluded.value, source = excluded.source",
    )?
    .execute(params![system_id, title, field, value, source])?;
    Ok(true)
}

/// Store `fields` from `source` ([`put_field`] each), the release date as
/// one: its month and day only where the year (and month) stored are its
/// own, so a date is never put together from two releases.
pub fn put_fields(
    conn: &Connection,
    system_id: i64,
    title: &str,
    fields: &[(&str, String)],
    source: &str,
) -> Result<bool, Error> {
    let stored = |f: &str| -> Result<Option<String>, Error> {
        Ok(conn
            .prepare_cached("SELECT value FROM game_meta WHERE system_id = ?1 AND title = ?2 AND field = ?3")?
            .query_row(params![system_id, title, f], |r| r.get(0))
            .optional()?)
    };
    let mut wrote = false;
    let date = ["releaseyear", "releasemonth", "releaseday"];
    for (f, v) in fields.iter().filter(|(f, _)| !date.contains(f)) {
        wrote |= put_field(conn, system_id, title, f, v, source, false)?;
    }
    // Where this source's date stops fitting what is stored, its own finer
    // parts (kept from when it held the coarser ones) go too.
    let mut fits = date.len();
    for (i, f) in date.iter().enumerate() {
        let Some((_, v)) = fields.iter().find(|(k, _)| k == f) else {
            fits = i;
            break;
        };
        wrote |= put_field(conn, system_id, title, f, v, source, false)?;
        if stored(f)?.as_ref() != Some(v) {
            // An earlier source's part stands: none of this one's finer
            // parts go with it.
            fits = i + 1;
            break;
        }
    }
    for f in &date[fits..] {
        conn.execute(
            "DELETE FROM game_meta WHERE system_id = ?1 AND title = ?2 AND field = ?3 AND source = ?4",
            params![system_id, title, f, source],
        )?;
    }
    Ok(wrote)
}

/// Release dates by region (`1994-04-18` or `1994`, a region's name; ``
/// none) as fields: `releases` lists them all (`1994-03-19 Japan;
/// 1994-04-18 USA`), and the earliest gives `releaseyear`, `releasemonth`
/// and `releaseday`, as far as it goes.
pub fn release_fields<'a>(
    dates: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<(&'static str, String)> {
    let mut list: Vec<(String, String)> = dates
        .into_iter()
        .map(|(d, r)| (d.trim().to_owned(), r.trim().to_owned()))
        .filter(|(d, _)| d.len() >= 4 && d[..4].bytes().all(|b| b.is_ascii_digit()))
        .collect();
    list.sort();
    list.dedup();
    let Some((first, _)) = list.first() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let parts: Vec<&str> = first.split('-').collect();
    let names = ["releaseyear", "releasemonth", "releaseday"];
    for (name, part) in names.iter().zip(&parts) {
        match part.parse::<u32>() {
            Ok(n) if n > 0 => out.push((*name, n.to_string())),
            _ => break,
        }
    }
    let shown: Vec<String> = list
        .iter()
        .map(|(d, r)| {
            if r.is_empty() {
                d.clone()
            } else {
                format!("{d} {r}")
            }
        })
        .collect();
    out.push(("releases", shown.join("; ")));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn had(v: &str, s: Option<&str>) -> Option<(String, Option<String>)> {
        Some((v.to_owned(), s.map(str::to_owned)))
    }

    #[test]
    fn earlier_sources_win_and_each_keeps_its_own() {
        let order = [LIBRETRO, WIKIDATA, SCREENSCRAPER, IGDB, "openvgdb", "launchbox"];
        for (i, a) in order.iter().enumerate() {
            for (j, b) in order.iter().enumerate() {
                let r = replaces(a, had("x", Some(b)).as_ref());
                // Wikipedia shares Wikidata's place; equal places don't swap.
                assert_eq!(r, i < j || a == b, "{a} over {b}");
            }
            assert!(replaces(a, None) && replaces(a, had("???", Some(LIBRETRO)).as_ref()));
        }
        assert!(
            !replaces(WIKIPEDIA, had("x", Some(WIKIDATA)).as_ref()),
            "same place"
        );
        // Old rows: libretro's or Wikidata's, so only those two replace them.
        assert!(replaces(LIBRETRO, had("x", None).as_ref()));
        assert!(replaces(WIKIDATA, had("x", None).as_ref()));
        assert!(!replaces(SCREENSCRAPER, had("x", None).as_ref()));
        assert!(!replaces("launchbox", had("x", None).as_ref()));
    }

    #[test]
    fn release_dates_give_the_earliest_and_the_list() {
        let f = release_fields([
            ("1994-04-18", "USA"),
            ("1994-03-19", "Japan"),
            ("bad", "x"),
            ("1994-04-18", "USA"),
        ]);
        let get = |k: &str| f.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("releaseyear"), Some("1994"));
        assert_eq!(get("releasemonth"), Some("3"));
        assert_eq!(get("releaseday"), Some("19"));
        assert_eq!(get("releases"), Some("1994-03-19 Japan; 1994-04-18 USA"));
        assert!(
            release_fields([("1991", "")])
                .iter()
                .all(|(n, _)| *n != "releasemonth")
        );
    }

    #[test]
    fn a_date_is_never_put_together_from_two_releases() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE game_meta (system_id INTEGER, title TEXT, field TEXT, value TEXT, source TEXT,
               PRIMARY KEY (system_id, title, field));
             INSERT INTO game_meta VALUES (1, 'G', 'releaseyear', '1995', 'libretro');
             INSERT INTO game_meta VALUES (1, 'G', 'releasemonth', '9', 'libretro');",
        )
        .unwrap();
        let wd = release_fields([("1995-07-21", "Japan")]);
        put_fields(&conn, 1, "G", &wd, WIKIDATA).unwrap();
        let day: Option<String> = conn
            .query_row(
                "SELECT value FROM game_meta WHERE field = 'releaseday'",
                [],
                |r| r.get(0),
            )
            .optional()
            .unwrap();
        assert_eq!(day, None, "July's day doesn't go with libretro's September");
        let releases: String = conn
            .query_row("SELECT value FROM game_meta WHERE field = 'releases'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(releases, "1995-07-21 Japan");
        put_fields(&conn, 1, "H", &wd, WIKIDATA).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM game_meta WHERE title = 'H' AND field LIKE 'release%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 4, "a game with no date gets all of it");
        // A day LaunchBox once put under libretro's year and month goes
        // when LaunchBox's own month isn't theirs.
        conn.execute(
            "INSERT INTO game_meta VALUES (1, 'G', 'releaseday', '27', 'launchbox')",
            [],
        )
        .unwrap();
        put_fields(&conn, 1, "G", &release_fields([("1995-06-27", "")]), "launchbox").unwrap();
        let day: Option<String> = conn
            .query_row(
                "SELECT value FROM game_meta WHERE title = 'G' AND field = 'releaseday'",
                [],
                |r| r.get(0),
            )
            .optional()
            .unwrap();
        assert_eq!(day, None);
    }

    #[test]
    fn put_field_writes_only_what_wins() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE game_meta (system_id INTEGER, title TEXT, field TEXT, value TEXT, source TEXT,
               PRIMARY KEY (system_id, title, field));",
        )
        .unwrap();
        let get = || -> (String, String) {
            conn.query_row("SELECT value, source FROM game_meta", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap()
        };
        assert!(put_field(&conn, 1, "G", "genre", "RPG", "launchbox", false).unwrap());
        assert!(put_field(&conn, 1, "G", "genre", "Role-playing", IGDB, false).unwrap());
        assert!(!put_field(&conn, 1, "G", "genre", "Action", "launchbox", false).unwrap());
        assert_eq!(get(), ("Role-playing".into(), IGDB.into()));
        assert!(
            !put_field(&conn, 1, "G", "genre", "RPG", LIBRETRO, true).unwrap(),
            "fill only"
        );
        assert!(put_field(&conn, 1, "G", "genre", "RPG", LIBRETRO, false).unwrap());
        assert!(
            !put_field(&conn, 1, "G", "genre", "???", LIBRETRO, false).unwrap(),
            "junk isn't stored"
        );
        assert_eq!(get(), ("RPG".into(), LIBRETRO.into()));
    }
}
