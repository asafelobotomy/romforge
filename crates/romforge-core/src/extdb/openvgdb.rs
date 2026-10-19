use super::*;

/// OpenVGDB comes as one SQLite file in a zip: copy it out and index the
/// hashes.
pub(super) fn build_openvgdb(zip: &Path, out: &Path) -> Result<i64, Error> {
    let mut archive = zip::ZipArchive::new(BufReader::new(std::fs::File::open(zip)?))?;
    let mut member = archive.by_name("openvgdb.sqlite")?;
    // Bounded: the real file is about 42 MB; a zip can claim to hold anything.
    let copied = std::io::copy(
        &mut member.by_ref().take(4 << 30),
        &mut std::fs::File::create(out)?,
    )?;
    if copied >= 4 << 30 {
        return Err(Error::Http(
            "openvgdb.sqlite in the zip is implausibly large".into(),
        ));
    }
    let conn = Connection::open(out)?;
    let n: i64 = conn
        .query_row("SELECT count(*) FROM ROMs", [], |r| r.get(0))
        .map_err(|e| Error::Http(format!("not an OpenVGDB file: {e}")))?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS romforge_sha1 ON ROMs (romHashSHA1);
         CREATE INDEX IF NOT EXISTS romforge_release_rom ON RELEASES (romID);",
    )?;
    Ok(n)
}

/// OpenVGDB by any of the game's ROMs (SHA1 and size). A SHA1 that is
/// more than one OpenVGDB ROM says nothing.
pub fn openvgdb_lookup(conn: &Connection, roms: &[(String, u64)]) -> Result<Option<Found>, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT r.romID, e.releaseDescription, e.releaseDeveloper, e.releasePublisher, e.releaseGenre,
                e.releaseDate
         FROM ROMs r CROSS JOIN RELEASES e ON e.romID = r.romID
         WHERE r.romHashSHA1 = ?1 AND (r.romSize IS NULL OR r.romSize = ?2)
         ORDER BY (e.releaseDescription IS NULL), e.releaseID",
    )?;
    for (sha1, size) in roms {
        let rows: Vec<(i64, [Option<String>; 5])> = stmt
            .query_map(params![sha1.to_ascii_uppercase(), *size as i64], |r| {
                Ok((r.get(0)?, [r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?]))
            })?
            .collect::<Result<_, _>>()?;
        let Some((rom, [desc, dev, publ, genre, date])) = rows.first().cloned() else {
            continue;
        };
        if rows.iter().any(|(r, _)| *r != rom) {
            continue;
        }
        let mut f = Found::default();
        let mut put = |k: &'static str, v: Option<String>| {
            if let Some(v) = v.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()) {
                f.fields.push((k, v));
            }
        };
        put("description", desc);
        put("developer", dev);
        put("publisher", publ);
        put(
            "genre",
            genre.map(|g| g.split(',').map(str::trim).collect::<Vec<_>>().join(", ")),
        );
        if let Some((y, m, d)) = date.as_deref().and_then(parse_us_date) {
            put("releaseyear", Some(y.to_string()));
            put("releasemonth", m.map(|m| m.to_string()));
            put("releaseday", d.map(|d| d.to_string()));
        }
        return Ok(Some(f));
    }
    Ok(None)
}
