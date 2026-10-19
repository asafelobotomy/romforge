use super::*;

/// field -> values for each of `qids`; an edition's empty fields are
/// filled from the game it is an edition of.
pub(super) fn details(
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
pub(super) fn inherit(
    fields: &mut BTreeMap<String, BTreeSet<String>>,
    from: &BTreeMap<String, BTreeSet<String>>,
) {
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
                    let label = row.get("vLabel").cloned().unwrap_or_else(|| v.clone());
                    if CREDIT_FIELDS.contains(f) && is_qid(v) && !is_qid(&label) {
                        out.entry(g.clone())
                            .or_default()
                            .entry(format!("credit:{f}"))
                            .or_default()
                            .insert(format!("{v}\t{label}"));
                    }
                    (*f, label)
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
