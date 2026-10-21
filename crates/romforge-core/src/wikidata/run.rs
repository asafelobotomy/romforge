use super::*;

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
        // Every name [`names_for`] tries (full title and edition-stripped
        // fallbacks) must be ready, or pick could settle "none" too early.
        todo.retain(|t| ready_for(t, &ready));
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
            let mut drop_credits = tx.prepare("DELETE FROM credits WHERE system_id = ?1 AND title = ?2")?;
            let mut credit = tx.prepare(
                "INSERT OR IGNORE INTO credits (system_id, title, field, qid, label) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (system_id, title, qid) in chunk {
                put.execute(params![system_id, title, "wikidata", qid])?;
                let Some(fields) = found.get(qid) else { continue };
                drop_credits.execute(params![system_id, title])?;
                for (field, values) in fields {
                    if let Some(f) = field.strip_prefix("credit:") {
                        for v in values {
                            if let Some((q, label)) = v.split_once('\t') {
                                credit.execute(params![system_id, title, f, q, label])?;
                            }
                        }
                        continue;
                    }
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

    // Pictures of who made them: a studio's logo, a person's photo.
    let wanted: Vec<String> = conn
        .prepare(if refresh {
            "SELECT DISTINCT qid FROM credits ORDER BY qid"
        } else {
            "SELECT DISTINCT c.qid FROM credits c
             WHERE NOT EXISTS (SELECT 1 FROM wikidata_pictures p WHERE p.qid = c.qid) ORDER BY c.qid"
        })?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for chunk in wanted.chunks(50) {
        if halted {
            break;
        }
        progress("pictures");
        match pictures(endpoint, chunk) {
            Ok(found) => {
                let tx = conn.transaction()?;
                for q in chunk {
                    let (url, kind) = match found.get(q) {
                        Some((u, k)) => (Some(u.as_str()), Some(*k)),
                        None => (None, None),
                    };
                    tx.execute(
                        concat!(
                            "INSERT INTO wikidata_pictures (qid, url, kind, checked_at) VALUES (?1, ?2, ?3, ",
                            crate::db::now!(),
                            ")
                         ON CONFLICT(qid) DO UPDATE SET url = excluded.url, kind = excluded.kind,
                           checked_at = excluded.checked_at,
                           path = CASE WHEN excluded.url IS wikidata_pictures.url THEN wikidata_pictures.path END"
                        ),
                        params![q, url, kind],
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
