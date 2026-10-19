use super::*;

/// Metadata and cached art for games whose name contains `query`
/// (case-insensitive; an exact name comes first).
pub fn info(conn: &Connection, query: &str) -> Result<Vec<GameInfo>, Error> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT s.key, t.system_id, t.title FROM (
             SELECT system_id, title FROM game_meta UNION SELECT system_id, title FROM art
         ) t JOIN systems s ON s.system_id = t.system_id
         WHERE t.title LIKE '%' || ?2 || '%' ESCAPE '\\'
         ORDER BY t.title <> ?1, t.title LIMIT 50",
    )?;
    let hits: Vec<(String, i64, String)> = stmt
        .query_map(params![query, crate::like_escape(query)], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<Result<_, _>>()?;
    let mut fields =
        conn.prepare("SELECT field, value FROM game_meta WHERE system_id = ?1 AND title = ?2")?;
    let mut art =
        conn.prepare("SELECT kind, path FROM art WHERE system_id = ?1 AND title = ?2 AND path IS NOT NULL")?;
    let mut out = Vec::new();
    for (key, system_id, title) in hits {
        let f: BTreeMap<String, String> = fields
            .query_map(params![system_id, title], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        let a: BTreeMap<ArtKind, PathBuf> = art
            .query_map(params![system_id, title], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .filter_map(|row| {
                let (k, p) = row.ok()?;
                Some((ArtKind::parse(&k)?, PathBuf::from(p)))
            })
            .collect();
        out.push(GameInfo {
            system: key,
            title,
            fields: f,
            art: a,
        });
    }
    Ok(out)
}
