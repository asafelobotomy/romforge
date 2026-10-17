//! Milestone 9, first stage: Wikidata (free, no account, CC0 data).
//!
//! For each platform with games you have, one SPARQL query lists the video
//! games on that platform (label, aliases, release dates; English or the
//! language-neutral "mul" labels Wikidata uses for names that are the same
//! everywhere, such as "Super Mario World"). A game is matched
//! when exactly one of them has the same title (case, punctuation, region
//! tags and a trailing ", The" ignored), narrowed by release year when
//! several do. Ambiguous and unmatched games are recorded as such for the
//! Search queue (milestone 8) to settle; nothing is guessed.
//!
//! For matched games: developer, publisher, country of origin, director,
//! composer, designer, genre, game modes, the series and its ordinal, IDs
//! for MobyGames, IGDB, HowLongToBeat and RetroAchievements, and the English
//! Wikipedia article. Then every member of those series, in order, for the
//! Series tab.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::{Connection, OptionalExtension, params};

use crate::{Error, fetch, systems};

pub const ENDPOINT: &str = "https://query.wikidata.org/sparql";

/// Item-valued properties stored by label, and their `game_meta` field.
pub const LABEL_FIELDS: &[(&str, &str)] = &[
    ("P178", "developer"),
    ("P123", "publisher"),
    ("P495", "country"),
    ("P57", "director"),
    ("P86", "composer"),
    ("P287", "designer"),
    ("P136", "genre"),
    ("P404", "game_mode"),
];

/// External-ID properties and their `game_meta` field.
pub const ID_FIELDS: &[(&str, &str)] = &[
    ("P11688", "mobygames_id"),
    ("P1933", "mobygames_slug"),
    ("P5794", "igdb_slug"),
    ("P2816", "hltb_id"),
    ("P11393", "retroachievements_id"),
];

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct WikidataReport {
    pub games: usize,
    pub matched: usize,
    pub ambiguous: usize,
    pub unmatched: usize,
    pub errors: Vec<String>,
}

/// A video game on one platform, as listed for matching.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Candidate {
    pub qid: String,
    pub label: String,
    pub years: BTreeSet<i32>,
    #[serde(skip)]
    pub names: BTreeSet<String>,
}

/// Lowercase letters and digits only, "&" as "and", words single-spaced;
/// region tags and a trailing article ("Zelda, The") handled first.
pub fn normalize(title: &str) -> String {
    let t = crate::view::search_title(title).replace('&', " and ");
    let mut out = String::new();
    for w in t.split(|c: char| !c.is_alphanumeric()) {
        if !w.is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&w.to_lowercase());
        }
    }
    out
}

/// Pick the one candidate for `title`, using `year` (release year from
/// other metadata) when several have the same name. `Err(n)` is how many
/// matched equally (0: none).
pub fn pick<'a>(
    title: &str,
    year: Option<i32>,
    candidates: &'a [Candidate],
) -> Result<&'a Candidate, Vec<&'a Candidate>> {
    let want = normalize(title);
    let named: Vec<&Candidate> = candidates.iter().filter(|c| c.names.contains(&want)).collect();
    match named.as_slice() {
        [one] => Ok(one),
        [] => Err(Vec::new()),
        many => {
            let by_year: Vec<&Candidate> = match year {
                Some(y) => many.iter().copied().filter(|c| c.years.contains(&y)).collect(),
                None => Vec::new(),
            };
            match by_year.as_slice() {
                [one] => Ok(one),
                _ => Err(many.to_vec()),
            }
        }
    }
}

fn sparql(endpoint: &str, query: &str) -> Result<Vec<HashMap<String, String>>, Error> {
    let url = format!("{endpoint}?format=json&query={}", fetch::encode_segment(query));
    let body = fetch::get(&url)?;
    parse_results(&body)
}

/// SPARQL JSON results as rows of variable -> value (IRIs of entities
/// shortened to their Q/P id).
pub fn parse_results(body: &[u8]) -> Result<Vec<HashMap<String, String>>, Error> {
    let v: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| Error::Http(format!("Wikidata answer: {e}")))?;
    let rows = v["results"]["bindings"]
        .as_array()
        .ok_or_else(|| Error::Http("Wikidata answer has no results".into()))?;
    Ok(rows
        .iter()
        .map(|row| {
            row.as_object()
                .into_iter()
                .flatten()
                .filter_map(|(k, cell)| {
                    let value = cell["value"].as_str()?;
                    let value = value
                        .strip_prefix("http://www.wikidata.org/entity/")
                        .unwrap_or(value);
                    Some((k.clone(), value.to_owned()))
                })
                .collect()
        })
        .collect())
}

fn year_of(date: &str) -> Option<i32> {
    date.get(..4)?.parse().ok()
}

/// Every video game Wikidata lists on `platform` (a Q id).
pub fn candidates(endpoint: &str, platform: &str) -> Result<Vec<Candidate>, Error> {
    let q = format!(
        "SELECT ?g ?label ?alias ?date WHERE {{
           ?g wdt:P31 wd:Q7889; wdt:P400 wd:{platform}; rdfs:label ?label.
           FILTER(LANG(?label) IN (\"en\", \"mul\"))
           OPTIONAL {{ ?g skos:altLabel ?alias. FILTER(LANG(?alias) IN (\"en\", \"mul\")) }}
           OPTIONAL {{ ?g wdt:P577 ?date }}
         }}"
    );
    let mut by_qid: BTreeMap<String, Candidate> = BTreeMap::new();
    for row in sparql(endpoint, &q)? {
        let Some(qid) = row.get("g") else { continue };
        let c = by_qid.entry(qid.clone()).or_insert_with(|| Candidate {
            qid: qid.clone(),
            ..Candidate::default()
        });
        if let Some(l) = row.get("label") {
            c.label = l.clone();
            c.names.insert(normalize(l));
        }
        if let Some(a) = row.get("alias") {
            c.names.insert(normalize(a));
        }
        if let Some(y) = row.get("date").and_then(|d| year_of(d)) {
            c.years.insert(y);
        }
    }
    Ok(by_qid.into_values().collect())
}

/// field -> values for each of `qids`.
fn details(
    endpoint: &str,
    qids: &[String],
) -> Result<HashMap<String, BTreeMap<String, BTreeSet<String>>>, Error> {
    let values: String = qids.iter().map(|q| format!("wd:{q} ")).collect();
    let props: String = LABEL_FIELDS
        .iter()
        .chain(ID_FIELDS)
        .map(|(p, _)| format!("wdt:{p} "))
        .collect();
    let q = format!(
        "SELECT ?g ?p ?v ?vLabel WHERE {{
           VALUES ?g {{ {values} }}
           {{ VALUES ?p {{ {props} }} ?g ?p ?v. }}
           UNION {{ ?g p:P179 ?st. ?st ps:P179 ?v. BIND(\"series\" AS ?p) }}
           UNION {{ ?g p:P179 ?st. ?st pq:P1545 ?v. BIND(\"series_ordinal\" AS ?p) }}
           UNION {{ ?a schema:about ?g; schema:isPartOf <https://en.wikipedia.org/>; schema:name ?v.
                    BIND(\"wikipedia\" AS ?p) }}
           SERVICE wikibase:label {{ bd:serviceParam wikibase:language \"en,mul\". }}
         }}"
    );
    let field_of: HashMap<String, &str> = LABEL_FIELDS
        .iter()
        .chain(ID_FIELDS)
        .map(|(p, f)| (format!("http://www.wikidata.org/prop/direct/{p}"), *f))
        .collect();
    let mut out: HashMap<String, BTreeMap<String, BTreeSet<String>>> = HashMap::new();
    for row in sparql(endpoint, &q)? {
        let (Some(g), Some(p), Some(v)) = (row.get("g"), row.get("p"), row.get("v")) else {
            continue;
        };
        let (field, value) = match p.as_str() {
            "series" => {
                let label = row.get("vLabel").cloned().unwrap_or_default();
                let fields = out.entry(g.clone()).or_default();
                fields.entry("series_qid".into()).or_default().insert(v.clone());
                ("series", label)
            }
            "series_ordinal" | "wikipedia" => (p.as_str(), v.clone()),
            other => match field_of.get(other) {
                Some(f) if LABEL_FIELDS.iter().any(|(_, lf)| lf == f) => {
                    (*f, row.get("vLabel").cloned().unwrap_or_else(|| v.clone()))
                }
                Some(f) => (*f, v.clone()),
                None => continue,
            },
        };
        // An unlabelled item comes back as its Q id; skip those.
        if value.is_empty() || (value.starts_with('Q') && value[1..].chars().all(|c| c.is_ascii_digit())) {
            continue;
        }
        out.entry(g.clone())
            .or_default()
            .entry(field.to_owned())
            .or_default()
            .insert(value);
    }
    Ok(out)
}

/// (series, member, title, ordinal, year)
type SeriesRow = (String, String, String, Option<String>, Option<i32>);

/// Members of `series` (Q ids).
fn series_members(endpoint: &str, series: &[String]) -> Result<Vec<SeriesRow>, Error> {
    let values: String = series.iter().map(|q| format!("wd:{q} ")).collect();
    let q = format!(
        "SELECT ?s ?m ?mLabel ?ord (MIN(?date) AS ?first) WHERE {{
           VALUES ?s {{ {values} }}
           ?m p:P179 ?st. ?st ps:P179 ?s.
           OPTIONAL {{ ?st pq:P1545 ?ord }}
           OPTIONAL {{ ?m wdt:P577 ?date }}
           SERVICE wikibase:label {{ bd:serviceParam wikibase:language \"en,mul\". }}
         }} GROUP BY ?s ?m ?mLabel ?ord"
    );
    Ok(sparql(endpoint, &q)?
        .into_iter()
        .filter_map(|r| {
            Some((
                r.get("s")?.clone(),
                r.get("m")?.clone(),
                r.get("mLabel")?.clone(),
                r.get("ord").cloned(),
                r.get("first").and_then(|d| year_of(d)),
            ))
        })
        .collect())
}

/// A libretro value romforge treats as missing ("Nintendo ???").
fn junk(v: &str) -> bool {
    v.trim().is_empty() || v.contains("???")
}

/// Match the games you have to Wikidata and store what it knows. Games
/// matched before are refreshed; ambiguous or unmatched ones are tried
/// again only with `refresh`.
pub fn enrich(
    conn: &mut Connection,
    endpoint: &str,
    games: &BTreeMap<(i64, String), Vec<String>>,
    refresh: bool,
    mut progress: impl FnMut(&'static str),
) -> Result<WikidataReport, Error> {
    let mut report = WikidataReport {
        games: games.len(),
        ..WikidataReport::default()
    };
    let mut by_system: BTreeMap<i64, Vec<&String>> = BTreeMap::new();
    for (system, title) in games.keys() {
        by_system.entry(*system).or_default().push(title);
    }
    let mut matched: Vec<(i64, String, String)> = Vec::new();
    // Once Wikidata says "slow down", nothing more is asked in this run.
    let mut halted = false;

    for (system_id, titles) in by_system {
        let Some(system) = systems::by_id(system_id) else {
            continue;
        };
        if system.wikidata.is_empty() {
            continue;
        }
        // Already settled (matched, or picked by hand) unless refreshing.
        let mut todo = Vec::new();
        for title in titles {
            let known: Option<(Option<String>, String)> = conn
                .query_row(
                    "SELECT qid, status FROM wikidata_match WHERE system_id = ?1 AND title = ?2",
                    params![system_id, title],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            match known {
                Some((Some(qid), status)) if status == "by_you" || !refresh => {
                    matched.push((system_id, title.clone(), qid));
                }
                Some((_, _)) if !refresh => {}
                _ => todo.push(title),
            }
        }
        if todo.is_empty() || halted {
            continue;
        }
        progress(system.key);
        let mut cands = Vec::new();
        for platform in system.wikidata {
            match candidates(endpoint, platform) {
                Ok(c) => cands.extend(c),
                Err(e) => {
                    halted |= fetch::is_rate_limited(&e);
                    report.errors.push(format!("{}: {e}", system.key));
                }
            }
            if halted {
                break;
            }
        }
        if cands.is_empty() {
            continue;
        }
        let tx = conn.transaction()?;
        for title in todo {
            let year: Option<i32> = tx
                .query_row(
                    "SELECT value FROM game_meta WHERE system_id = ?1 AND title = ?2 AND field = 'releaseyear'",
                    params![system_id, title],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .and_then(|y| y.parse().ok());
            let (qid, status, others) = match pick(title, year, &cands) {
                Ok(c) => (Some(c.qid.clone()), "matched", Vec::new()),
                Err(many) if many.is_empty() => (None, "none", Vec::new()),
                Err(many) => (None, "ambiguous", many.into_iter().cloned().collect()),
            };
            tx.execute(
                "INSERT INTO wikidata_match (system_id, title, qid, status, candidates, checked_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                 ON CONFLICT(system_id, title) DO UPDATE SET qid = excluded.qid, status = excluded.status,
                   candidates = excluded.candidates, checked_at = excluded.checked_at",
                params![
                    system_id,
                    title,
                    qid,
                    status,
                    serde_json::to_string(&others).unwrap_or_default()
                ],
            )?;
            if let Some(q) = qid {
                matched.push((system_id, title.clone(), q));
            }
        }
        tx.commit()?;
    }

    // Details, 40 games per query.
    let mut series: BTreeSet<String> = BTreeSet::new();
    for chunk in matched.chunks(40) {
        if halted {
            break;
        }
        progress("details");
        let qids: Vec<String> = chunk.iter().map(|m| m.2.clone()).collect();
        let found = match details(endpoint, &qids) {
            Ok(f) => f,
            Err(e) => {
                halted |= fetch::is_rate_limited(&e);
                report.errors.push(e.to_string());
                continue;
            }
        };
        let tx = conn.transaction()?;
        {
            let mut current =
                tx.prepare("SELECT value FROM game_meta WHERE system_id = ?1 AND title = ?2 AND field = ?3")?;
            let mut put = tx.prepare(
                "INSERT INTO game_meta (system_id, title, field, value) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(system_id, title, field) DO UPDATE SET value = excluded.value",
            )?;
            for (system_id, title, qid) in chunk {
                put.execute(params![system_id, title, "wikidata", qid])?;
                let Some(fields) = found.get(qid) else { continue };
                for (field, values) in fields {
                    let value = values.iter().cloned().collect::<Vec<_>>().join(", ");
                    // Fill what libretro left empty (or marked unknown); Wikidata's
                    // own fields are always its.
                    let shared = matches!(field.as_str(), "developer" | "publisher" | "genre");
                    if shared {
                        let had: Option<String> = current
                            .query_row(params![system_id, title, field], |r| r.get(0))
                            .optional()?;
                        if had.is_some_and(|h| !junk(&h)) {
                            continue;
                        }
                    }
                    if field == "series_qid" {
                        series.extend(values.iter().cloned());
                    }
                    put.execute(params![system_id, title, field, value])?;
                }
            }
        }
        tx.commit()?;
    }

    // Series members, 20 series per query.
    let series: Vec<String> = series.into_iter().collect();
    for chunk in series.chunks(20) {
        if halted {
            break;
        }
        progress("series");
        match series_members(endpoint, chunk) {
            Ok(rows) => {
                let tx = conn.transaction()?;
                for (s, m, title, ord, year) in rows {
                    tx.execute(
                        "INSERT INTO series_members (series_qid, qid, title, ordinal, year) VALUES (?1, ?2, ?3, ?4, ?5)
                         ON CONFLICT(series_qid, qid) DO UPDATE SET title = excluded.title,
                           ordinal = excluded.ordinal, year = excluded.year",
                        params![s, m, title, ord, year],
                    )?;
                }
                tx.commit()?;
            }
            Err(e) => {
                halted |= fetch::is_rate_limited(&e);
                report.errors.push(e.to_string());
            }
        }
    }

    let mut stmt = conn.prepare("SELECT status FROM wikidata_match WHERE system_id = ?1 AND title = ?2")?;
    for (system_id, title) in games.keys() {
        match stmt
            .query_row(params![system_id, title], |r| r.get::<_, String>(0))
            .optional()?
            .as_deref()
        {
            Some("matched" | "by_you") => report.matched += 1,
            Some("ambiguous") => report.ambiguous += 1,
            Some(_) => report.unmatched += 1,
            None => {}
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(qid: &str, names: &[&str], years: &[i32]) -> Candidate {
        Candidate {
            qid: qid.into(),
            label: names[0].into(),
            years: years.iter().copied().collect(),
            names: names.iter().map(|n| normalize(n)).collect(),
        }
    }

    #[test]
    fn titles_normalize() {
        assert_eq!(
            normalize("Legend of Zelda, The - A Link to the Past (USA)"),
            "the legend of zelda a link to the past"
        );
        assert_eq!(
            normalize("The Legend of Zelda: A Link to the Past"),
            "the legend of zelda a link to the past"
        );
        assert_eq!(normalize("Ghouls 'n Ghosts & More"), "ghouls n ghosts and more");
    }

    #[test]
    fn picks_one_or_says_why_not() {
        let c = vec![
            cand("Q1", &["Super Mario World"], &[1990, 1991]),
            cand("Q2", &["Doom"], &[1993]),
            cand("Q3", &["Doom"], &[2016]),
            cand(
                "Q4",
                &["The Legend of Zelda: A Link to the Past", "A Link to the Past"],
                &[1991],
            ),
        ];
        assert_eq!(pick("Super Mario World (USA)", None, &c).unwrap().qid, "Q1");
        assert_eq!(
            pick("Legend of Zelda, The - A Link to the Past (USA)", None, &c)
                .unwrap()
                .qid,
            "Q4"
        );
        assert_eq!(pick("Doom (USA)", Some(1993), &c).unwrap().qid, "Q2");
        assert_eq!(
            pick("Doom (USA)", None, &c).unwrap_err().len(),
            2,
            "ambiguous without a year"
        );
        assert!(pick("Unknown Game (USA)", None, &c).unwrap_err().is_empty());
    }

    #[test]
    fn parses_sparql_json() {
        let body = br#"{"results":{"bindings":[{"g":{"type":"uri","value":"http://www.wikidata.org/entity/Q123"},"label":{"type":"literal","value":"Doom"}}]}}"#;
        let rows = parse_results(body).unwrap();
        assert_eq!(rows[0]["g"], "Q123");
        assert_eq!(rows[0]["label"], "Doom");
    }
}
