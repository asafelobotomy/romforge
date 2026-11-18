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

/// Articles asked for in one request (the most the API gives intros of).
pub const BATCH: usize = 20;

/// Fill the descriptions of `games` from Wikipedia (`base`,
/// [`WIKIPEDIA`]), in `lang` where it has the article: those with one,
/// whose description is missing or from a later source, and not read from
/// that article before (unless `refresh`). [`BATCH`] articles per request
/// to each wiki. Returns how many were filled; stops when told to slow
/// down, or once `cancel` is set (what was read is kept).
pub fn descriptions(
    conn: &Connection,
    base: &str,
    lang: &str,
    games: &BTreeMap<(i64, String), Vec<String>>,
    refresh: bool,
    cancel: &crate::job::Cancel,
    errors: &mut Vec<String>,
) -> Result<usize, Error> {
    // What to read, by wiki: (system, game, article).
    let mut todo: BTreeMap<String, Vec<(i64, &str, String)>> = BTreeMap::new();
    for (system_id, title) in games.keys() {
        let Some((lang, name)) = article(conn, *system_id, title, lang)? else {
            continue;
        };
        let had = field(conn, *system_id, title, "description")?;
        if !crate::enrich::replaces(crate::enrich::WIKIPEDIA, had.as_ref()) {
            continue;
        }
        let read_as = format!("{lang}:{name}");
        let same = field(conn, *system_id, title, "description_article")?.is_some_and(|(v, _)| v == read_as);
        if same && !refresh {
            continue;
        }
        todo.entry(lang).or_default().push((*system_id, title, name));
    }
    let mut filled = 0;
    'wikis: for (lang, wanted) in &todo {
        for chunk in wanted.chunks(BATCH) {
            if cancel.is_set() {
                break 'wikis;
            }
            let titles: Vec<String> = chunk.iter().map(|(_, _, n)| fetch::encode_segment(n)).collect();
            let url = format!(
                "{}/w/api.php?action=query&format=json&formatversion=2&redirects=1\
                 &prop=extracts%7Cpageprops%7Cinfo&exintro=1&explaintext=1&exlimit=max\
                 &ppprop=disambiguation&inprop=url&titles={}",
                base.replace("{lang}", lang),
                titles.join("%7C")
            );
            let pages = match fetch::get_small(&url).map(|b| extracts(&b)) {
                Ok(Some(p)) => p,
                Ok(None) => {
                    errors.push(format!("Wikipedia ({lang}): an answer romforge can't read"));
                    continue;
                }
                Err(e) => {
                    errors.push(format!("Wikipedia: {e}"));
                    if fetch::is_rate_limited(&e) {
                        break 'wikis;
                    }
                    continue;
                }
            };
            for (system_id, title, name) in chunk {
                // Not in the answer at all: asked again next time.
                let Some(page) = pages.get(name.as_str()) else {
                    continue;
                };
                let put = |f: &str, v: &str| {
                    crate::enrich::put_field(conn, *system_id, title, f, v, crate::enrich::WIKIPEDIA, false)
                };
                // No such article, a disambiguation page, or no text: read,
                // not asked again unless refreshing.
                if let Some((text, url)) = page {
                    if put("description", text)? {
                        filled += 1;
                    }
                    put("description_url", url)?;
                    put("description_licence", LICENCE)?;
                }
                put("description_article", &format!("{lang}:{name}"))?;
            }
        }
    }
    Ok(filled)
}

/// What an answer says of each title asked (through its normalizing and
/// redirects): the intro's plain text and the page's address, `None` for
/// no such page, a disambiguation page or one with no text. `None` for an
/// answer that isn't one.
pub(super) fn extracts(body: &[u8]) -> Option<HashMap<String, Option<(String, String)>>> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    let q = v.get("query")?;
    let hops = |key: &str| -> HashMap<String, String> {
        q[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|h| Some((h["from"].as_str()?.to_owned(), h["to"].as_str()?.to_owned())))
            .collect()
    };
    let (normalized, redirects) = (hops("normalized"), hops("redirects"));
    let pages: HashMap<&str, &serde_json::Value> = q["pages"]
        .as_array()?
        .iter()
        .filter_map(|p| Some((p["title"].as_str()?, p)))
        .collect();
    let mut out: HashMap<String, Option<(String, String)>> = pages
        .iter()
        .map(|(t, p)| ((*t).to_owned(), read_page(p)))
        .collect();
    // A title asked under another spelling, or a redirect, reads as the
    // page it leads to (one hop each, as the API makes them).
    for (from, to) in normalized.iter().chain(redirects.iter()) {
        let end = redirects.get(to).unwrap_or(to);
        if let Some(p) = pages.get(end.as_str()) {
            out.insert(from.clone(), read_page(p));
        }
    }
    Some(out)
}

/// One page of an answer: `None` when missing, a disambiguation page or
/// without text.
fn read_page(p: &serde_json::Value) -> Option<(String, String)> {
    if p.get("missing").is_some() || p["pageprops"].get("disambiguation").is_some() {
        return None;
    }
    let text = p["extract"].as_str()?.trim();
    let url = p["fullurl"].as_str()?;
    (!text.is_empty()).then(|| (text.to_owned(), url.to_owned()))
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

    /// The answer the test server gives for `path`: each title asked,
    /// one missing, one a disambiguation page, one a redirect.
    fn answer(path: &str) -> String {
        let lang = path.trim_start_matches('/').split('/').next().unwrap_or("");
        let titles = path.split("titles=").nth(1).unwrap_or("");
        let (mut pages, mut redirects) = (Vec::new(), Vec::new());
        for t in titles.split("%7C").map(fetch::decode_segment) {
            let page = match t.as_str() {
                "Missing" => serde_json::json!({"title": t, "missing": true}),
                "Ambiguous" => {
                    serde_json::json!({"title": t, "pageprops": {"disambiguation": ""}, "extract": "May mean:"})
                }
                "Old G" => {
                    redirects.push(serde_json::json!({"from": t, "to": "Game G"}));
                    serde_json::json!({"title": "Game G", "extract": "From the redirect.", "fullurl": "https://en.wikipedia.org/wiki/Game_G"})
                }
                _ => {
                    serde_json::json!({"title": t, "extract": format!("From {lang}/{t}."), "fullurl": format!("https://{lang}.wikipedia.org/wiki/X")})
                }
            };
            pages.push(page);
        }
        serde_json::json!({"query": {"redirects": redirects, "pages": pages}}).to_string()
    }

    /// The matched game's article is read once, in the language set, and
    /// a description from an earlier source stands; one request per wiki.
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
                let body = answer(&path);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
             INSERT INTO game_meta VALUES (1, 'F', 'wikipedia', 'Ambiguous', 'wikidata');
             INSERT INTO game_meta VALUES (1, 'G', 'wikipedia', 'Old G', 'wikidata');",
        )
        .unwrap();
        let games: BTreeMap<(i64, String), Vec<String>> = ["A", "B", "C", "D", "E", "F", "G"]
            .into_iter()
            .map(|t| ((1, t.to_owned()), Vec::new()))
            .collect();
        let mut errors = Vec::new();
        let cancel = crate::job::Cancel::new();
        assert_eq!(
            descriptions(&conn, &base, "fr", &games, false, &cancel, &mut errors).unwrap(),
            3
        );
        assert_eq!(asked.lock().unwrap().len(), 2, "one request each to fr and en");
        let desc = |t: &str| -> Option<String> {
            conn.query_row(
                "SELECT value FROM game_meta WHERE title = ?1 AND field = 'description'",
                [t],
                |r| r.get(0),
            )
            .optional()
            .unwrap()
        };
        assert_eq!(desc("A").as_deref(), Some("From fr/Jeu A."), "French article");
        assert_eq!(desc("B").as_deref(), Some("Mine."), "libretro's stands");
        assert_eq!(
            desc("C").as_deref(),
            Some("From en/Game C."),
            "LaunchBox's gives way; English when there's no French"
        );
        assert_eq!(desc("E"), None, "no such article");
        assert_eq!(desc("F"), None, "a disambiguation page");
        assert_eq!(desc("G").as_deref(), Some("From the redirect."));
        descriptions(&conn, &base, "fr", &games, false, &cancel, &mut errors).unwrap();
        assert_eq!(
            asked.lock().unwrap().len(),
            2,
            "not asked again, a missing or disambiguation page neither"
        );
        assert!(errors.is_empty(), "{errors:?}");
        // Stopped: nothing is asked.
        cancel.cancel();
        descriptions(&conn, &base, "fr", &games, true, &cancel, &mut errors).unwrap();
        assert_eq!(asked.lock().unwrap().len(), 2, "stopped before asking");
    }
}
