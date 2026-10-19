//! Everything known about one row.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, params};

use super::links::{links, patch_links, shown_value};
use super::{Credit, Derived, GameDetail, LibraryView, Relation, RowFile, RowStatus, SeriesEntry, library};
use crate::{Error, systems};

/// Who made a game (or, for a translation or hack, its base game).
fn credits(conn: &Connection, system_id: i64, titles: &[&str]) -> Result<Vec<Credit>, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT c.field, c.label, c.qid, p.path, p.kind, p.url FROM credits c
         LEFT JOIN wikidata_pictures p ON p.qid = c.qid
         WHERE c.system_id = ?1 AND c.title = ?2",
    )?;
    for title in titles {
        let mut out: Vec<Credit> = stmt
            .query_map(params![system_id, title], |r| {
                let url: Option<String> = r.get(5)?;
                Ok(Credit {
                    role: r.get(0)?,
                    name: r.get(1)?,
                    qid: r.get(2)?,
                    picture: r
                        .get::<_, Option<String>>(3)?
                        .filter(|p| std::path::Path::new(p).exists()),
                    kind: r.get(4)?,
                    source: url.and_then(|u| {
                        let file = u.rsplit_once("Special:FilePath/")?.1;
                        Some(format!("https://commons.wikimedia.org/wiki/File:{file}"))
                    }),
                })
            })?
            .collect::<Result<_, _>>()?;
        if !out.is_empty() {
            let order = |r: &str| crate::wikidata::CREDIT_FIELDS.iter().position(|f| *f == r);
            out.sort_by(|a, b| (order(&a.role), &a.name).cmp(&(order(&b.role), &b.name)));
            return Ok(out);
        }
    }
    Ok(Vec::new())
}

/// Everything known about one row of [`library`].
pub fn detail(conn: &Connection, id: &str) -> Result<GameDetail, Error> {
    detail_in(conn, &library(conn)?, id)
}

/// [`detail`] with the library already built (it costs a pass over every
/// file: build it once for many rows).
pub fn detail_in(conn: &Connection, view: &LibraryView, id: &str) -> Result<GameDetail, Error> {
    let row = view
        .rows
        .iter()
        .find(|r| r.id == id)
        .cloned()
        .ok_or_else(|| Error::Dat(format!("no library row {id:?}")))?;
    let mut fields = BTreeMap::new();
    let mut art = BTreeMap::new();
    let mut art_sources = BTreeMap::new();
    let mut field_sources = BTreeMap::new();
    let mut dats = Vec::new();
    // A derivative: what its files were named as.
    let mut proof = None;
    let derived: Option<Derived> = if row.status == RowStatus::Derivative {
        let mut stmt = conn.prepare(
            "SELECT m.base, m.relation, m.tag, m.proof FROM manual_ids m
             JOIN files f ON f.sha1 = m.sha1 AND f.size = m.size
             WHERE f.file_id = ?1 AND m.base IS NOT NULL",
        )?;
        row.file_ids.iter().find_map(|id| {
            stmt.query_row([id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .ok()
            .and_then(|(base, rel, tag, by)| {
                proof = by;
                Some(Derived {
                    relation: Relation::parse(&rel)?,
                    base,
                    tag,
                })
            })
        })
    } else {
        None
    };
    if let Some(system_id) = row.system_id {
        // Its own metadata and art first; a derivative's gaps from its base.
        let titles: Vec<&str> = std::iter::once(row.title.as_str())
            .chain(derived.as_ref().map(|d| d.base.as_str()))
            .collect();
        for title in titles {
            for r in conn
                .prepare("SELECT field, value, source FROM game_meta WHERE system_id = ?1 AND title = ?2")?
                .query_map(params![system_id, title], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                })?
            {
                let (k, v, src) = r?;
                if fields.contains_key(&k) {
                    continue;
                }
                if let Some(v) = shown_value(&v) {
                    if let Some(src) = src {
                        field_sources.insert(k.clone(), src);
                    }
                    fields.insert(k, v);
                }
            }
            for r in conn
                .prepare(
                    "SELECT kind, path, coalesce(source, 'libretro') FROM art
                     WHERE system_id = ?1 AND title = ?2 AND path IS NOT NULL",
                )?
                .query_map(params![system_id, title], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })?
            {
                let (k, p, src) = r?;
                if art.contains_key(&k) {
                    continue;
                }
                art_sources.insert(k.clone(), src);
                art.insert(k, p);
            }
        }
        dats = conn
            .prepare("SELECT DISTINCT d.name FROM games g JOIN dats d ON d.dat_id = g.dat_id WHERE g.name = ?1 AND d.system_id = ?2 ORDER BY d.name")?
            .query_map(params![row.title, system_id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
    }
    let mut stmt = conn.prepare(
        "SELECT path, member, status, size, sha1, header_kind, id_kind, id_serial, id_title, id_region, id_size,
                id_crc
             FROM files WHERE file_id = ?1",
    )?;
    let mut files = Vec::new();
    for id in &row.file_ids {
        let f = stmt.query_row([id], |r| {
            let path: String = r.get(0)?;
            let member: String = r.get(1)?;
            Ok(RowFile {
                path: if member.is_empty() {
                    path
                } else {
                    format!("{path}#{member}")
                },
                status: r.get(2)?,
                size: r.get(3)?,
                sha1: r.get(4)?,
                header_kind: r.get(5)?,
                game_id: crate::ident::GameId::from_columns(
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                    r.get(11)?,
                ),
            })
        })?;
        files.push(f);
    }
    let links = if row.system_id.is_some() && row.status != RowStatus::Unknown {
        links(&row.title, row.system_name.as_deref(), &fields)
    } else {
        Vec::new()
    };
    let mut series = Vec::new();
    // Wikidata's series, in order; else games of the same franchise that
    // romforge has metadata for.
    if let Some(series_qids) = fields.get("series_qid") {
        let mine: BTreeSet<String> = conn
            .prepare("SELECT qid FROM wikidata_match WHERE qid IS NOT NULL")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let me = fields.get("wikidata");
        let mut stmt =
            conn.prepare("SELECT qid, title, ordinal, year FROM series_members WHERE series_qid = ?1")?;
        let mut seen = BTreeSet::new();
        for s in series_qids.split(", ") {
            for r in stmt.query_map([s], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            })? {
                let (qid, title, ordinal, year) = r?;
                if !seen.insert(qid.clone()) {
                    continue;
                }
                series.push(SeriesEntry {
                    current: me == Some(&qid),
                    in_library: mine.contains(&qid),
                    system: String::new(),
                    title,
                    year: year.map(|y| y.to_string()),
                    ordinal,
                });
            }
        }
        let num = |o: &Option<String>| o.as_deref().and_then(|v| v.parse::<f64>().ok());
        series.sort_by(|a, b| match (num(&a.ordinal), num(&b.ordinal)) {
            (Some(x), Some(y)) => x.total_cmp(&y),
            _ => (a.year.is_none(), &a.year, &a.title).cmp(&(b.year.is_none(), &b.year, &b.title)),
        });
    } else if let Some(franchise) = fields.get("franchise") {
        let mut stmt = conn.prepare(
            "SELECT f.system_id, f.title,
                    (SELECT value FROM game_meta y WHERE y.system_id = f.system_id
                       AND y.title = f.title AND y.field = 'releaseyear')
             FROM game_meta f WHERE f.field = 'franchise' AND f.value = ?1",
        )?;
        for r in stmt.query_map([franchise], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })? {
            let (system_id, title, year) = r?;
            series.push(SeriesEntry {
                current: Some(system_id) == row.system_id && title == row.title,
                in_library: true,
                system: systems::by_id(system_id).map_or(String::new(), |s| s.key.to_owned()),
                title,
                year,
                ordinal: None,
            });
        }
        series.sort_by(|a, b| {
            (a.year.is_none(), &a.year, &a.title).cmp(&(b.year.is_none(), &b.year, &b.title))
        });
    }
    let mut patched = None;
    for f in &files {
        if let Some(p) = crate::patcher::made_by(conn, &f.sha1, f.size)? {
            patched = Some(p);
            break;
        }
    }
    let (patch_links, patches) = match row.status {
        RowStatus::Verified | RowStatus::ByYou | RowStatus::Missing => (
            patch_links(&row.title),
            crate::patcher::paired_with(conn, row.system_id, &row.title)?,
        ),
        // A patch row: that patch.
        RowStatus::Patch => (
            Vec::new(),
            crate::patcher::patch_at(conn, row.id.strip_prefix("p:").unwrap_or(&row.id))?
                .into_iter()
                .collect(),
        ),
        _ => (Vec::new(), Vec::new()),
    };
    let credits = match row.system_id {
        Some(sid) => {
            let titles: Vec<&str> = std::iter::once(row.title.as_str())
                .chain(derived.as_ref().map(|d| d.base.as_str()))
                .collect();
            credits(conn, sid, &titles)?
        }
        None => Vec::new(),
    };
    Ok(GameDetail {
        patched,
        patch_links,
        patches,
        proof,
        credits,
        name: crate::names::parse(&row.title),
        derived,
        row,
        fields,
        art,
        art_sources,
        field_sources,
        files,
        dats,
        links,
        series,
    })
}
