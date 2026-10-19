use super::*;

/// Each item's logo (P154) on Wikimedia Commons, as its file URL.
pub(crate) fn logos(endpoint: &str, qids: &[String]) -> Result<HashMap<String, String>, Error> {
    let values: String = qids.iter().map(|q| format!("wd:{q} ")).collect();
    let q = format!(
        "SELECT ?e ?logo WHERE {{
           VALUES ?e {{ {values} }}
           ?e wdt:P154 ?logo.
         }}"
    );
    let mut out = HashMap::new();
    for row in sparql(endpoint, &q)? {
        if let (Some(e), Some(l)) = (row.get("e"), row.get("logo")) {
            out.entry(e.clone()).or_insert_with(|| l.clone());
        }
    }
    Ok(out)
}

/// Each item's picture on Wikimedia Commons: its logo (P154), else its
/// image (P18, a person's photo), as (file URL, "logo" or "photo").
pub(super) fn pictures(
    endpoint: &str,
    qids: &[String],
) -> Result<HashMap<String, (String, &'static str)>, Error> {
    let values: String = qids.iter().map(|q| format!("wd:{q} ")).collect();
    let q = format!(
        "SELECT ?e ?logo ?img WHERE {{
           VALUES ?e {{ {values} }}
           OPTIONAL {{ ?e wdt:P154 ?logo }}
           OPTIONAL {{ ?e wdt:P18 ?img }}
         }}"
    );
    let mut out: HashMap<String, (String, &'static str)> = HashMap::new();
    for row in sparql(endpoint, &q)? {
        let Some(e) = row.get("e") else { continue };
        if let Some(l) = row.get("logo") {
            out.insert(e.clone(), (l.clone(), "logo"));
        } else if let Some(i) = row.get("img") {
            out.entry(e.clone()).or_insert((i.clone(), "photo"));
        }
    }
    Ok(out)
}

/// Download the pictures not cached yet into `dir` (`<qid>.<ext>`), a
/// small thumbnail each (Commons scales SVG logos to PNG). Returns how many
/// it fetched; errors are listed, the rest goes on.
pub fn fetch_pictures(
    conn: &Connection,
    dir: &std::path::Path,
    errors: &mut Vec<String>,
) -> Result<usize, Error> {
    let todo: Vec<(String, String)> = conn
        .prepare("SELECT qid, url FROM wikidata_pictures WHERE url IS NOT NULL AND path IS NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut n = 0;
    for (qid, url) in todo {
        let sep = if url.contains('?') { '&' } else { '?' };
        let body = match fetch::get(&format!("{url}{sep}width=240")) {
            Ok(b) => b,
            Err(e) => {
                let limited = fetch::is_rate_limited(&e);
                errors.push(format!("picture of {qid}: {e}"));
                if limited {
                    break;
                }
                continue;
            }
        };
        let Some(ext) = crate::enrich::image_ext(&body) else {
            errors.push(format!("picture of {qid}: not an image romforge shows"));
            continue;
        };
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{qid}.{ext}"));
        std::fs::write(&path, &body)?;
        conn.execute(
            "UPDATE wikidata_pictures SET path = ?2 WHERE qid = ?1",
            params![qid, path.to_string_lossy()],
        )?;
        n += 1;
    }
    Ok(n)
}
