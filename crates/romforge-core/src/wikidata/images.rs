use super::*;

/// Each item's logos (P154) on Wikimedia Commons, as file URLs: SVGs
/// first, then by name, so the same one is shown every time.
pub(crate) fn logos(endpoint: &str, qids: &[String]) -> Result<HashMap<String, Vec<String>>, Error> {
    let values: String = qids.iter().map(|q| format!("wd:{q} ")).collect();
    let q = format!(
        "SELECT ?e ?logo WHERE {{
           VALUES ?e {{ {values} }}
           ?e wdt:P154 ?logo.
         }}"
    );
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for row in sparql(endpoint, &q)? {
        if let (Some(e), Some(l)) = (row.get("e"), row.get("logo")) {
            out.entry(e.clone()).or_default().push(l.clone());
        }
    }
    for files in out.values_mut() {
        files.sort_by_key(|f| (!f.to_ascii_lowercase().ends_with(".svg"), f.clone()));
        files.dedup();
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

/// How long a picture that failed to download is left alone: one Commons
/// doesn't have (or that isn't a picture romforge shows) for a month, one
/// that failed otherwise (offline, a server error) for a day.
const GONE_RETRY: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 3600);
const FAILED_RETRY: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

/// The marker of a failed download: `<dir>/<qid>.failed`, written when it
/// failed (its modification time) and holding how long to wait, in seconds.
/// A file beside the pictures, like the platform icons' `.missing`, so the
/// database needs no column for it.
fn failed_marker(dir: &std::path::Path, qid: &str) -> std::path::PathBuf {
    dir.join(format!("{qid}.failed"))
}

/// Whether `qid`'s picture failed to download too recently to ask again.
fn backing_off(dir: &std::path::Path, qid: &str) -> bool {
    let marker = failed_marker(dir, qid);
    let Ok(meta) = std::fs::metadata(&marker) else {
        return false;
    };
    let wait = std::fs::read_to_string(&marker)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .map_or(FAILED_RETRY, std::time::Duration::from_secs);
    meta.modified()
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|since| since < wait)
}

fn mark_failed(dir: &std::path::Path, qid: &str, wait: std::time::Duration) {
    // A marker that can't be written only costs asking again.
    let _ = crate::atomic::write(&failed_marker(dir, qid), wait.as_secs().to_string().as_bytes());
}

/// Download the pictures not cached yet into `dir` (`<qid>.<ext>`), a
/// small thumbnail each (Commons scales SVG logos to PNG). Returns how many
/// it fetched; errors are listed, the rest goes on. One that failed is
/// marked and left alone for a while (a month, or a day),
/// unless the server asked to slow down.
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
        if backing_off(dir, &qid) {
            continue;
        }
        let sep = if url.contains('?') { '&' } else { '?' };
        let body = match fetch::get_image(&format!("{url}{sep}width=240")) {
            Ok(Some(b)) => b,
            Ok(None) => {
                errors.push(format!("picture of {qid}: not on Commons (HTTP 404)"));
                mark_failed(dir, &qid, GONE_RETRY);
                continue;
            }
            Err(e) => {
                let limited = fetch::is_rate_limited(&e);
                errors.push(format!("picture of {qid}: {e}"));
                if limited {
                    break;
                }
                mark_failed(dir, &qid, FAILED_RETRY);
                continue;
            }
        };
        let Some(ext) = crate::enrich::image_ext(&body) else {
            errors.push(format!("picture of {qid}: not an image romforge shows"));
            mark_failed(dir, &qid, GONE_RETRY);
            continue;
        };
        let path = dir.join(format!("{qid}.{ext}"));
        crate::atomic::write(&path, &body)?;
        let _ = std::fs::remove_file(failed_marker(dir, &qid));
        conn.execute(
            "UPDATE wikidata_pictures SET path = ?2 WHERE qid = ?1",
            params![qid, path.to_string_lossy()],
        )?;
        n += 1;
    }
    Ok(n)
}
