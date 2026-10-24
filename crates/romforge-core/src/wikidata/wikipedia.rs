//! Descriptions from Wikipedia (CC BY-SA 4.0): the summary of the
//! article Wikidata links the matched game to, in the language set (else
//! English). Never a title search, so it is as sure as the Wikidata match
//! (house rule 8). Stored with its link and licence, which Details shows.

use super::*;

/// Where Wikipedia's REST API is, `{lang}` for the language's wiki.
pub const WIKIPEDIA: &str = "https://{lang}.wikipedia.org";

/// The licence of Wikipedia's text.
pub const LICENCE: &str = "CC BY-SA 4.0";

fn field(
    conn: &Connection,
    system_id: i64,
    title: &str,
    f: &str,
) -> Result<Option<(String, Option<String>)>, Error> {
    Ok(conn
        .prepare_cached(
            "SELECT value, source FROM game_meta WHERE system_id = ?1 AND title = ?2 AND field = ?3",
        )?
        .query_row(params![system_id, title, f], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?)
}

/// The article to read for a game: `wikipedia_local` (`<lang>:<title>`)
/// in `lang`, else the English `wikipedia`.
fn article(
    conn: &Connection,
    system_id: i64,
    title: &str,
    lang: &str,
) -> Result<Option<(String, String)>, Error> {
    if let Some((v, Some(s))) = field(conn, system_id, title, "wikipedia_local")?
        && s == crate::enrich::WIKIDATA
        && let Some((l, name)) = v.split_once(':')
        && l == lang
    {
        return Ok(Some((lang.to_owned(), name.to_owned())));
    }
    // Only Wikidata's link: another source's (LaunchBox's, by title) may
    // be a list or the wrong game.
    Ok(field(conn, system_id, title, "wikipedia")?
        .filter(|(_, s)| s.as_deref() == Some(crate::enrich::WIKIDATA))
        .map(|(v, _)| ("en".to_owned(), v)))
}

/// Fill the descriptions of `games` from Wikipedia (`base`,
/// [`WIKIPEDIA`]), in `lang` where it has the article: those with one, whose description is missing or
/// from a later source, and not read from that article before (unless
/// `refresh`). Returns how many were filled; stops when told to slow down.
pub fn descriptions(
    conn: &Connection,
    base: &str,
    lang: &str,
    games: &BTreeMap<(i64, String), Vec<String>>,
    refresh: bool,
    errors: &mut Vec<String>,
) -> Result<usize, Error> {
    let mut filled = 0;
    for (system_id, title) in games.keys() {
        let Some((lang, name)) = article(conn, *system_id, title, lang)? else {
            continue;
        };
        let read_as = format!("{lang}:{name}");
        let had = field(conn, *system_id, title, "description")?;
        if !crate::enrich::replaces(crate::enrich::WIKIPEDIA, had.as_ref()) {
            continue;
        }
        let same = field(conn, *system_id, title, "description_article")?.is_some_and(|(v, _)| v == read_as);
        if same && !refresh {
            continue;
        }
        let url = format!(
            "{}/api/rest_v1/page/summary/{}",
            base.replace("{lang}", &lang),
            fetch::encode_segment(&name.replace(' ', "_"))
        );
        let put = |f: &str, v: &str| {
            crate::enrich::put_field(conn, *system_id, title, f, v, crate::enrich::WIKIPEDIA, false)
        };
        let body = match fetch::get_small_optional(&url) {
            Ok(Some(b)) => b,
            // No such article: read, not asked again unless refreshing.
            Ok(None) => {
                put("description_article", &read_as)?;
                continue;
            }
            Err(e) => {
                let stop = fetch::is_rate_limited(&e);
                errors.push(format!("Wikipedia: {e}"));
                if stop {
                    break;
                }
                continue;
            }
        };
        let Some((text, page)) = summary(&body) else {
            // A disambiguation page, or no text: the same.
            put("description_article", &read_as)?;
            continue;
        };
        if put("description", &text)? {
            filled += 1;
        }
        put("description_url", &page)?;
        put("description_licence", LICENCE)?;
        put("description_article", &read_as)?;
    }
    Ok(filled)
}

/// A summary's plain text and its page's address; `None` for a
/// disambiguation page or one with no text.
pub(super) fn summary(body: &[u8]) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    if v["type"].as_str() == Some("disambiguation") {
        return None;
    }
    let text = v["extract"].as_str()?.trim();
    let page = v["content_urls"]["desktop"]["page"].as_str()?;
    (!text.is_empty()).then(|| (text.to_owned(), page.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratings_and_dates_come_out_plain() {
        assert_eq!(rating("pegi", "PEGI 12").as_deref(), Some("12"));
        assert_eq!(rating("esrb", "ESRB Teen").as_deref(), Some("T"));
        assert_eq!(rating("esrb", "Everyone 10+").as_deref(), Some("E10+"));
        assert_eq!(rating("cero", "CERO A").as_deref(), Some("A"));
        assert_eq!(rating("esrb", "something else"), None);
        let dates: BTreeSet<String> = [
            "1994-03-19T00:00:00Z|11|Japan",
            "1994-01-01T00:00:00Z|9|Europe",
            "1993-06-01T00:00:00Z|8|",
            "2006-12-08T00:00:00Z|11|Europe|Q8079",
            "1994-02-02T00:00:00Z|11|Japan|Q172742",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let f = super::super::run::releases(&dates, &["Q172742"]);
        let get = |k: &str| f.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
        assert_eq!(
            get("releases"),
            Some("1994 Europe; 1994-02-02 Japan; 1994-03-19 Japan"),
            "a decade's precision, and the re-release on another platform, are dropped"
        );
        assert_eq!(get("releaseyear"), Some("1994"));
        assert_eq!(get("releasemonth"), None, "the earliest is only a year");
    }

    /// The matched game's article is read once, in the language set, and
    /// a description from an earlier source stands.
    #[test]
    fn descriptions_come_from_the_linked_article() {
        use std::io::{BufRead, BufReader, Write};
        use std::sync::{Arc, Mutex};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/{{lang}}", listener.local_addr().unwrap());
        let asked: Arc<Mutex<Vec<String>>> = Arc::default();
        let log = asked.clone();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let path = line.split_whitespace().nth(1).unwrap_or("").to_owned();
                let mut h = String::new();
                while reader.read_line(&mut h).unwrap_or(0) > 2 {
                    h.clear();
                }
                log.lock().unwrap().push(path.clone());
                let (status, body) = if path.contains("Missing") {
                    ("404 Not Found", String::new())
                } else if path.contains("Ambiguous") {
                    (
                        "200 OK",
                        r#"{"type":"disambiguation","extract":"May mean:"}"#.to_owned(),
                    )
                } else {
                    (
                        "200 OK",
                        format!(
                            r#"{{"type":"standard","extract":"From {path}.","content_urls":{{"desktop":{{"page":"https://fr.wikipedia.org/wiki/X"}}}}}}"#
                        ),
                    )
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE game_meta (system_id INTEGER, title TEXT, field TEXT, value TEXT, source TEXT,
               PRIMARY KEY (system_id, title, field));
             INSERT INTO game_meta VALUES (1, 'A', 'wikipedia', 'Game A', 'wikidata');
             INSERT INTO game_meta VALUES (1, 'A', 'wikipedia_local', 'fr:Jeu A', 'wikidata');
             INSERT INTO game_meta VALUES (1, 'B', 'wikipedia', 'Game B', 'wikidata');
             INSERT INTO game_meta VALUES (1, 'B', 'description', 'Mine.', 'libretro');
             INSERT INTO game_meta VALUES (1, 'C', 'wikipedia', 'Game C', 'wikidata');
             INSERT INTO game_meta VALUES (1, 'D', 'wikipedia', 'List of games', 'launchbox');
             INSERT INTO game_meta VALUES (1, 'C', 'description', 'Theirs.', 'launchbox');
             INSERT INTO game_meta VALUES (1, 'E', 'wikipedia', 'Missing', 'wikidata');
             INSERT INTO game_meta VALUES (1, 'F', 'wikipedia', 'Ambiguous', 'wikidata');",
        )
        .unwrap();
        let games: BTreeMap<(i64, String), Vec<String>> = ["A", "B", "C", "D", "E", "F"]
            .into_iter()
            .map(|t| ((1, t.to_owned()), Vec::new()))
            .collect();
        let mut errors = Vec::new();
        assert_eq!(
            descriptions(&conn, &base, "fr", &games, false, &mut errors).unwrap(),
            2
        );
        let desc = |t: &str| -> String {
            conn.query_row(
                "SELECT value FROM game_meta WHERE title = ?1 AND field = 'description'",
                [t],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(
            desc("A"),
            "From /fr/api/rest_v1/page/summary/Jeu_A.",
            "French article"
        );
        assert_eq!(desc("B"), "Mine.", "libretro's stands");
        assert!(
            desc("C").starts_with("From /en/"),
            "LaunchBox's gives way; English when there's no French"
        );
        let n = asked.lock().unwrap().len();
        descriptions(&conn, &base, "fr", &games, false, &mut errors).unwrap();
        assert_eq!(
            asked.lock().unwrap().len(),
            n,
            "not asked again, a missing or disambiguation page neither"
        );
        assert!(errors.is_empty(), "{errors:?}");
    }
}
