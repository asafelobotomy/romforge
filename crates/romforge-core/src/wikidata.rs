//! Milestone 9, first stage: Wikidata (free, no account, CC0 data).
//!
//! For the games you have, Wikidata's own search (run inside the SPARQL
//! service, 20 titles per query) finds the video games on that platform
//! with those words; each comes back with its labels, aliases and release
//! dates (English, or the language-neutral "mul" labels Wikidata uses for
//! names that are the same everywhere, such as "Super Mario World"). What
//! was found and which titles were searched are kept per platform, so a
//! run stopped by a rate limit resumes where it stopped. A game is matched
//! when exactly one candidate has the same title (case, punctuation, region
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
    /// For an edition (Q60997816), the game it is an edition of (P629).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<String>,
}

/// Wikidata's classes for a game: video game, and video game edition (a
/// "Director's Cut" or "Greatest Hits" release with an item of its own).
const GAME: &str = "Q7889";
const EDITION: &str = "Q60997816";

/// romforge's one match key for titles from other sources (Wikidata,
/// OpenVGDB, LaunchBox): lowercase letters and digits only, accents folded
/// ("Pokémon" = "Pokemon"), "&" as "and", words single-spaced; region tags
/// and a trailing article ("Zelda, The") handled first.
pub fn normalize(title: &str) -> String {
    let t = crate::extdb::fold_accents(&crate::view::search_title(title)).replace('&', " and ");
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
    // The full name first; only when nothing has it, the name without an
    // edition tag ("Resident Evil - Director's Cut - Dual Shock Ver." is
    // the Director's Cut). An ambiguous full name stays ambiguous.
    for want in names_for(title) {
        let named: Vec<&Candidate> = candidates.iter().filter(|c| c.names.contains(&want)).collect();
        match named.as_slice() {
            [] => continue,
            [one] => return Ok(one),
            many => {
                // A game beats its own edition of the same name.
                let games: Vec<&Candidate> = many
                    .iter()
                    .copied()
                    .filter(|c| !c.work.as_ref().is_some_and(|w| many.iter().any(|o| &o.qid == w)))
                    .collect();
                if let [one] = games.as_slice() {
                    return Ok(one);
                }
                let many = games.as_slice();
                // An item whose own label is the name beats one that only
                // has it as an alias (Rondo of Blood is also "Dracula X").
                let own: Vec<&Candidate> = many
                    .iter()
                    .copied()
                    .filter(|c| normalize(&c.label) == want)
                    .collect();
                if let [one] = own.as_slice() {
                    return Ok(one);
                }
                let by_year: Vec<&Candidate> = match year {
                    Some(y) => many.iter().copied().filter(|c| c.years.contains(&y)).collect(),
                    None => Vec::new(),
                };
                return match by_year.as_slice() {
                    [one] => Ok(one),
                    _ => Err(many.to_vec()),
                };
            }
        }
    }
    Err(Vec::new())
}

/// The names a DAT title is looked for under: its own, then without the
/// edition tags it ends in, one by one ("Dual Shock Ver.", "Greatest Hits",
/// "Platinum", ...). Only parts that are clearly an edition go, so a
/// subtitle that names the game ("The Empire Strikes Back") never does.
pub fn names_for(title: &str) -> Vec<String> {
    let mut out = vec![normalize(title)];
    let mut t = crate::view::search_title(title);
    while let Some((head, last)) = t.rsplit_once(" - ") {
        if !is_edition(last) {
            break;
        }
        t = head.to_owned();
        let n = normalize(&t);
        if !n.is_empty() && !out.contains(&n) {
            out.push(n);
        }
    }
    out.retain(|n| !n.is_empty());
    out
}

/// A title part that names an edition of the game, not the game.
fn is_edition(part: &str) -> bool {
    let p = part.trim().to_lowercase();
    p.ends_with(" ver.")
        || p == "ver."
        || [
            "dual shock",
            "greatest hits",
            "platinum",
            "the best",
            "player's choice",
            "players choice",
            "big in japan",
            "limited edition",
            "collector's edition",
            "nintendo selects",
        ]
        .iter()
        .any(|w| p.contains(w))
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

/// Titles looked up per SPARQL query.
const SEARCH_BATCH: usize = 20;

/// What Wikidata's search is given for a normalized title: its words, minus
/// stray letters (the "n" of "Ghouls 'n Ghosts"), which the search may
/// split differently. Matching itself stays strict, in [`pick`].
fn search_words(name: &str) -> String {
    let words: Vec<&str> = name
        .split(' ')
        .filter(|w| w.chars().count() > 1 || w.chars().all(|c| c.is_ascii_digit()))
        .collect();
    if words.is_empty() {
        name.to_owned()
    } else {
        words.join(" ")
    }
}

/// The video games on `platform` (a Q id) that Wikidata's search finds for
/// `names` (normalized titles: letters, digits and spaces only), with every
/// English or "mul" label and alias, for [`pick`] to match strictly.
pub fn candidates(endpoint: &str, platform: &str, names: &[String]) -> Result<Vec<Candidate>, Error> {
    let searches: String = names
        .iter()
        .map(|n| {
            format!(
                "\"{} haswbstatement:P400={platform} haswbstatement:P31={GAME}|P31={EDITION}\" ",
                search_words(n)
            )
        })
        .collect();
    let q = format!(
        "SELECT ?g ?label ?alias ?date ?work WHERE {{
           VALUES ?search {{ {searches} }}
           SERVICE wikibase:mwapi {{
             bd:serviceParam wikibase:api \"Search\"; wikibase:endpoint \"www.wikidata.org\";
                             mwapi:srsearch ?search; mwapi:srlimit \"20\"; wikibase:limit \"once\".
             ?g wikibase:apiOutputItem mwapi:title.
           }}
           VALUES ?kind {{ wd:{GAME} wd:{EDITION} }}
           ?g wdt:P31 ?kind; wdt:P400 wd:{platform}; rdfs:label ?label.
           FILTER(LANG(?label) IN (\"en\", \"mul\"))
           OPTIONAL {{ ?g skos:altLabel ?alias. FILTER(LANG(?alias) IN (\"en\", \"mul\")) }}
           OPTIONAL {{ ?g wdt:P577 ?date }}
           OPTIONAL {{ ?g wdt:P31 wd:{EDITION}; wdt:P629 ?work }}
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
            if c.label.is_empty() || l.as_str() < c.label.as_str() {
                c.label = l.clone();
            }
            c.names.insert(normalize(l));
        }
        if let Some(a) = row.get("alias") {
            c.names.insert(normalize(a));
        }
        if let Some(y) = row.get("date").and_then(|d| year_of(d)) {
            c.years.insert(y);
        }
        if let Some(w) = row.get("work") {
            c.work = Some(w.clone());
        }
    }
    Ok(by_qid.into_values().collect())
}

/// Keep what one search found on `platform`, and that `names` were searched.
fn remember(
    conn: &mut Connection,
    platform: &str,
    names: &[String],
    found: &[Candidate],
) -> Result<(), Error> {
    let tx = conn.transaction()?;
    for c in found {
        tx.execute(
            "INSERT INTO wikidata_candidates (platform, qid, label, names, years, work)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(platform, qid) DO UPDATE SET label = excluded.label, names = excluded.names,
               years = excluded.years, work = excluded.work",
            params![
                platform,
                c.qid,
                c.label,
                serde_json::to_string(&c.names).unwrap_or_default(),
                serde_json::to_string(&c.years).unwrap_or_default(),
                c.work
            ],
        )?;
    }
    for n in names {
        tx.execute(
            concat!(
                "INSERT INTO wikidata_searched (platform, name, searched_at)
             VALUES (?1, ?2, ",
                crate::db::now!(),
                ")
             ON CONFLICT(platform, name) DO UPDATE SET searched_at = excluded.searched_at"
            ),
            params![platform, n],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Names searched on `platform`.
fn searched(conn: &Connection, platform: &str) -> Result<BTreeSet<String>, Error> {
    let mut stmt = conn.prepare("SELECT name FROM wikidata_searched WHERE platform = ?1")?;
    let names = stmt
        .query_map(params![platform], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(names)
}

/// Every candidate kept for `platforms`, one per item.
fn kept(conn: &Connection, platforms: &[&str]) -> Result<Vec<Candidate>, Error> {
    let mut stmt =
        conn.prepare("SELECT qid, label, names, years, work FROM wikidata_candidates WHERE platform = ?1")?;
    let mut by_qid: BTreeMap<String, Candidate> = BTreeMap::new();
    for platform in platforms {
        let rows = stmt.query_map(params![platform], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?;
        for row in rows {
            let (qid, label, names, years, work) = row?;
            let c = by_qid.entry(qid.clone()).or_insert_with(|| Candidate {
                qid,
                label,
                ..Candidate::default()
            });
            // Kept before accents were folded: normalized again.
            c.names.extend(
                serde_json::from_str::<BTreeSet<String>>(&names)
                    .unwrap_or_default()
                    .iter()
                    .map(|n| normalize(n)),
            );
            c.years
                .extend(serde_json::from_str::<BTreeSet<i32>>(&years).unwrap_or_default());
            c.work = c.work.take().or(work);
        }
    }
    Ok(by_qid.into_values().collect())
}

/// field -> values for each of `qids`; an edition's empty fields are
/// filled from the game it is an edition of.
fn details(
    endpoint: &str,
    qids: &[String],
) -> Result<HashMap<String, BTreeMap<String, BTreeSet<String>>>, Error> {
    let mut out = item_details(endpoint, qids)?;
    let works: BTreeMap<String, String> = out
        .iter_mut()
        .filter_map(|(q, f)| Some((q.clone(), f.remove("work")?.pop_first()?)))
        .collect();
    if works.is_empty() {
        return Ok(out);
    }
    let wanted: Vec<String> = works
        .values()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut of_work = item_details(endpoint, &wanted)?;
    for (q, w) in works {
        let Some(from) = of_work.get_mut(&w) else { continue };
        from.remove("work");
        let fields = out.entry(q).or_default();
        inherit(fields, from);
    }
    Ok(out)
}

/// Fill the fields `fields` lacks from `from` (the series and its Q id go
/// together).
fn inherit(fields: &mut BTreeMap<String, BTreeSet<String>>, from: &BTreeMap<String, BTreeSet<String>>) {
    let has_series = fields.contains_key("series");
    for (f, v) in from {
        let series = matches!(f.as_str(), "series" | "series_qid" | "series_ordinal");
        if (series && has_series) || fields.contains_key(f) {
            continue;
        }
        fields.insert(f.clone(), v.clone());
    }
}

fn item_details(
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
           UNION {{ ?g wdt:P31 wd:{EDITION}; wdt:P629 ?v. BIND(\"work\" AS ?p) }}
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
            "work" => {
                let fields = out.entry(g.clone()).or_default();
                fields.entry("work".into()).or_default().insert(v.clone());
                continue;
            }
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
        if todo.is_empty() {
            continue;
        }
        // Look up only the titles not searched yet on each platform item
        // (`refresh` forgets earlier searches); what was found is kept, so
        // a run cut short resumes where it stopped.
        let names: BTreeSet<String> = todo.iter().flat_map(|t| names_for(t)).collect();
        let mut ready = names.clone();
        for platform in system.wikidata {
            if refresh && !halted {
                let mut forget =
                    conn.prepare("DELETE FROM wikidata_searched WHERE platform = ?1 AND name = ?2")?;
                for n in &names {
                    forget.execute(params![platform, n])?;
                }
            }
            let done = searched(conn, platform)?;
            let missing: Vec<String> = names.difference(&done).cloned().collect();
            if !missing.is_empty() && !halted {
                progress(system.key);
            }
            for batch in missing.chunks(SEARCH_BATCH) {
                if halted {
                    break;
                }
                match candidates(endpoint, platform, batch) {
                    Ok(found) => remember(conn, platform, batch, &found)?,
                    Err(e) => {
                        halted |= fetch::is_rate_limited(&e);
                        report.errors.push(format!("{}: {e}", system.key));
                    }
                }
            }
            let done = searched(conn, platform)?;
            ready.retain(|n| done.contains(n));
        }
        // Titles not searched everywhere yet wait for the next run.
        todo.retain(|t| {
            let n = normalize(t);
            n.is_empty() || ready.contains(&n)
        });
        if todo.is_empty() {
            continue;
        }
        let cands = kept(conn, system.wikidata)?;
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
                concat!(
                    "INSERT INTO wikidata_match (system_id, title, qid, status, candidates, checked_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ",
                    crate::db::now!(),
                    ")
                 ON CONFLICT(system_id, title) DO UPDATE SET qid = excluded.qid, status = excluded.status,
                   candidates = excluded.candidates, checked_at = excluded.checked_at"
                ),
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
                 ON CONFLICT(system_id, title, field) DO UPDATE SET value = excluded.value, source = NULL",
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
            work: None,
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
    fn own_label_beats_an_alias() {
        let c = [
            cand(
                "Q2421908",
                &["Castlevania: Rondo of Blood", "Castlevania: Dracula X"],
                &[1993, 1995],
            ),
            cand("Q2607689", &["Castlevania: Dracula X"], &[1995]),
        ];
        assert_eq!(
            pick("Castlevania - Dracula X (USA)", None, &c).unwrap().qid,
            "Q2607689"
        );
    }

    #[test]
    fn edition_tags_fall_back_to_the_game() {
        let c = [
            cand("Q1", &["Resident Evil: Director's Cut"], &[1997]),
            cand("Q2", &["Super Star Wars"], &[1992]),
        ];
        assert_eq!(
            pick("Resident Evil - Director's Cut - Dual Shock Ver. (USA)", None, &c)
                .unwrap()
                .qid,
            "Q1"
        );
        assert!(
            pick("Super Star Wars - The Empire Strikes Back (USA)", None, &c).is_err(),
            "a subtitle that names the game is kept"
        );
        assert_eq!(
            names_for("Resident Evil - Director's Cut - Dual Shock Ver. (USA)"),
            [
                "resident evil director s cut dual shock ver",
                "resident evil director s cut"
            ]
        );
        assert_eq!(
            names_for("Pokemon - Gold Version (USA)"),
            ["pokemon gold version"]
        );
    }

    #[test]
    fn an_edition_inherits_what_it_lacks() {
        let set = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
        let mut edition = BTreeMap::from([
            ("publisher".to_string(), set(&["Capcom"])),
            ("wikipedia".to_string(), set(&["Resident Evil (1996 video game)"])),
        ]);
        let game = BTreeMap::from([
            ("publisher".to_string(), set(&["Virgin"])),
            ("developer".to_string(), set(&["Capcom"])),
            ("series".to_string(), set(&["Resident Evil"])),
            ("series_qid".to_string(), set(&["Q1"])),
            ("series_ordinal".to_string(), set(&["1"])),
        ]);
        inherit(&mut edition, &game);
        assert_eq!(
            edition["publisher"],
            set(&["Capcom"]),
            "the edition's own value stays"
        );
        assert_eq!(edition["developer"], set(&["Capcom"]));
        assert_eq!(edition["series_ordinal"], set(&["1"]));

        let mut own_series = BTreeMap::from([("series".to_string(), set(&["Other"]))]);
        inherit(&mut own_series, &game);
        assert!(
            !own_series.contains_key("series_ordinal"),
            "a series comes whole or not at all"
        );
    }

    #[test]
    fn accents_fold() {
        let c = [cand("Q1", &["Pokémon Emerald Version"], &[2004])];
        assert_eq!(
            pick("Pokemon - Emerald Version (USA, Europe)", None, &c)
                .unwrap()
                .qid,
            "Q1"
        );
    }

    #[test]
    fn a_game_beats_its_own_edition() {
        let game = cand("Q1", &["Tomba!"], &[1997]);
        let mut edition = cand("Q2", &["Tomba!"], &[1998]);
        edition.work = Some("Q1".into());
        let other = cand("Q3", &["Tomba!"], &[1999]);
        assert_eq!(
            pick("Tomba! (USA)", None, &[game.clone(), edition.clone()])
                .unwrap()
                .qid,
            "Q1"
        );
        assert_eq!(
            pick("Tomba! (USA)", None, &[game, edition, other])
                .unwrap_err()
                .len(),
            2,
            "two games of that name stay ambiguous"
        );
    }

    #[test]
    fn search_words_drop_stray_letters() {
        assert_eq!(search_words("ghouls n ghosts"), "ghouls ghosts");
        assert_eq!(search_words("final fantasy 6"), "final fantasy 6");
        assert_eq!(search_words("x"), "x");
    }

    #[test]
    fn parses_sparql_json() {
        let body = br#"{"results":{"bindings":[{"g":{"type":"uri","value":"http://www.wikidata.org/entity/Q123"},"label":{"type":"literal","value":"Doom"}}]}}"#;
        let rows = parse_results(body).unwrap();
        assert_eq!(rows[0]["g"], "Q123");
        assert_eq!(rows[0]["label"], "Doom");
    }
}
