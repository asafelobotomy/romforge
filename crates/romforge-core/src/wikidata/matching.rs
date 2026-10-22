use super::*;

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

/// True when every name [`names_for`] looks up for `title` is in `ready`
/// (searched on every platform item). Empty titles are treated as ready.
pub(super) fn ready_for(title: &str, ready: &BTreeSet<String>) -> bool {
    let names = names_for(title);
    names.is_empty() || names.iter().all(|n| ready.contains(n))
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

pub(super) fn sparql(endpoint: &str, query: &str) -> Result<Vec<HashMap<String, String>>, Error> {
    let url = format!("{endpoint}?format=json&query={}", fetch::encode_segment(query));
    let body = fetch::get_small(&url)?;
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

/// A Wikidata item id: `Q` and digits.
pub(crate) fn is_qid(s: &str) -> bool {
    s.len() > 1 && s.starts_with('Q') && s[1..].chars().all(|c| c.is_ascii_digit())
}

pub(super) fn year_of(date: &str) -> Option<i32> {
    date.get(..4)?.parse().ok()
}

/// Titles looked up per SPARQL query.
pub(super) const SEARCH_BATCH: usize = 20;

/// What Wikidata's search is given for a normalized title: its words, minus
/// stray letters (the "n" of "Ghouls 'n Ghosts"), which the search may
/// split differently. Matching itself stays strict, in [`pick`].
pub(super) fn search_words(name: &str) -> String {
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
pub(super) fn remember(
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
pub(super) fn searched(conn: &Connection, platform: &str) -> Result<BTreeSet<String>, Error> {
    let mut stmt = conn.prepare("SELECT name FROM wikidata_searched WHERE platform = ?1")?;
    let names = stmt
        .query_map(params![platform], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(names)
}

/// Every candidate kept for `platforms`, one per item.
pub(super) fn kept(conn: &Connection, platforms: &[&str]) -> Result<Vec<Candidate>, Error> {
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
