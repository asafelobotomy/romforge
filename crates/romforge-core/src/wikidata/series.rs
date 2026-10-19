use super::*;

/// (series, member, title, ordinal, year)
type SeriesRow = (String, String, String, Option<String>, Option<i32>);

/// Members of `series` (Q ids).
pub(super) fn series_members(endpoint: &str, series: &[String]) -> Result<Vec<SeriesRow>, Error> {
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
pub(super) fn junk(v: &str) -> bool {
    v.trim().is_empty() || v.contains("???")
}
