use super::*;

/// LaunchBox's Metadata.xml, streamed out of the zip into a small index of
/// the platforms romforge knows.
pub(super) fn build_launchbox(zip: &Path, out: &Path, progress: &mut impl FnMut(&str)) -> Result<i64, Error> {
    let platforms: BTreeSet<&str> = systems::SYSTEMS
        .iter()
        .flat_map(|s| s.launchbox.iter().copied())
        .collect();
    let mut archive = zip::ZipArchive::new(BufReader::new(std::fs::File::open(zip)?))?;
    let member = archive.by_name("Metadata.xml")?;
    let mut conn = Connection::open(out)?;
    conn.execute_batch(
        "PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF;
         CREATE TABLE games (
            id INTEGER PRIMARY KEY, platform TEXT NOT NULL, name TEXT NOT NULL,
            overview TEXT, developer TEXT, publisher TEXT, genres TEXT,
            release_date TEXT, release_year TEXT, max_players TEXT, cooperative TEXT,
            esrb TEXT, rating TEXT, rating_count TEXT, wikipedia_url TEXT, video_url TEXT);
         CREATE TABLE names (id INTEGER NOT NULL, key TEXT NOT NULL, region TEXT);
         CREATE TABLE images (id INTEGER NOT NULL, file TEXT NOT NULL, type TEXT NOT NULL,
            region TEXT, crc32 TEXT);",
    )?;
    let tx = conn.transaction()?;
    let mut games = 0i64;
    {
        let mut put_game = tx.prepare(
            "INSERT OR REPLACE INTO games VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        )?;
        let mut put_name = tx.prepare("INSERT INTO names (id, key, region) VALUES (?1, ?2, ?3)")?;
        let mut put_image =
            tx.prepare("INSERT INTO images (id, file, type, region, crc32) VALUES (?1, ?2, ?3, ?4, ?5)")?;
        each_record(member, |tag, f| {
            let get = |k: &str| f.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
            let id: Option<i64> = get("DatabaseID").and_then(|v| v.parse().ok());
            let Some(id) = id else { return Ok(()) };
            match tag {
                "Game" => {
                    let (Some(platform), Some(name)) = (get("Platform"), get("Name")) else {
                        return Ok(());
                    };
                    if !platforms.contains(platform) {
                        return Ok(());
                    }
                    put_game.execute(params![
                        id,
                        platform,
                        name,
                        get("Overview"),
                        get("Developer"),
                        get("Publisher"),
                        get("Genres"),
                        get("ReleaseDate"),
                        get("ReleaseYear"),
                        get("MaxPlayers"),
                        get("Cooperative"),
                        get("ESRB"),
                        get("CommunityRating"),
                        get("CommunityRatingCount"),
                        get("WikipediaURL"),
                        get("VideoURL"),
                    ])?;
                    put_name.execute(params![
                        id,
                        crate::wikidata::normalize(name),
                        Option::<&str>::None
                    ])?;
                    games += 1;
                    if games % 20_000 == 0 {
                        progress(&format!("reading: {games} games"));
                    }
                }
                "GameAlternateName" => {
                    if let Some(n) = get("AlternateName") {
                        put_name.execute(params![id, crate::wikidata::normalize(n), get("Region")])?;
                    }
                }
                "GameImage" => {
                    if let (Some(file), Some(kind)) = (get("FileName"), get("Type")) {
                        put_image.execute(params![id, file, kind, get("Region"), get("CRC32")])?;
                    }
                }
                _ => {}
            }
            Ok(())
        })?;
    }
    progress("indexing");
    tx.execute_batch(
        "DELETE FROM names WHERE id NOT IN (SELECT id FROM games);
         DELETE FROM images WHERE id NOT IN (SELECT id FROM games);
         CREATE INDEX names_key ON names (key);
         CREATE INDEX images_id ON images (id);",
    )?;
    tx.commit()?;
    conn.execute_batch("VACUUM;")?;
    Ok(games)
}

/// Call `each` with (element, [(child, text)]) for every record element of
/// a LaunchBox XML file (the children of its root).
fn each_record(
    input: impl Read,
    mut each: impl FnMut(&str, &[(String, String)]) -> Result<(), Error>,
) -> Result<(), Error> {
    let xml_err = |e: quick_xml::Error| Error::Dat(format!("LaunchBox XML: {e}"));
    let mut reader = quick_xml::Reader::from_reader(BufReader::with_capacity(1 << 20, input));
    let mut buf = Vec::new();
    let mut depth = 0usize;
    let mut record = String::new();
    let mut child = String::new();
    let mut text = String::new();
    let mut fields: Vec<(String, String)> = Vec::new();
    loop {
        match reader.read_event_into(&mut buf).map_err(xml_err)? {
            Event::Start(e) => {
                depth += 1;
                let name = e.local_name().as_ref().to_owned();
                match depth {
                    2 => {
                        record = name;
                        fields.clear();
                    }
                    3 => {
                        child = name;
                        text.clear();
                    }
                    _ => {}
                }
            }
            Event::Text(t) if depth == 3 => text.push_str(&t.xml_content(XmlVersion::Implicit1_0)),
            Event::CData(t) if depth == 3 => text.push_str(&t.into_inner()),
            Event::GeneralRef(r) if depth == 3 => {
                if let Some(c) = r.resolve_char_ref().map_err(xml_err)? {
                    text.push(c);
                } else if let Some(s) = quick_xml::escape::resolve_predefined_entity(&r) {
                    text.push_str(s);
                }
            }
            Event::End(_) => {
                match depth {
                    3 => {
                        let v = text.trim();
                        if !v.is_empty() {
                            fields.push((std::mem::take(&mut child), v.to_owned()));
                        }
                    }
                    2 => each(&record, &fields)?,
                    _ => {}
                }
                depth = depth.saturating_sub(1);
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(())
}

/// "Aug 13, 1991", "Aug 1991" or "1991".
pub(super) fn parse_us_date(s: &str) -> Option<(i32, Option<u32>, Option<u32>)> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let words: Vec<&str> = s.split([' ', ',']).filter(|w| !w.is_empty()).collect();
    let year: i32 = words.last()?.parse().ok().filter(|y| (1950..2100).contains(y))?;
    let month = words
        .first()
        .and_then(|m| MONTHS.iter().position(|x| m.to_ascii_lowercase().starts_with(x)))
        .map(|i| i as u32 + 1);
    let day = if words.len() == 3 {
        words[1].parse().ok()
    } else {
        None
    };
    Some((year, month, day))
}

/// LaunchBox by title on the system's platforms: only when exactly one game
/// has that name there. `title` is the DAT name (its region tag picks the
/// images' region).
pub fn launchbox_lookup(
    conn: &Connection,
    system: &systems::System,
    title: &str,
    images_base: &str,
) -> Result<Option<Found>, Error> {
    if system.launchbox.is_empty() {
        return Ok(None);
    }
    let platforms = serde_json::to_string(system.launchbox).unwrap_or_default();
    let key = crate::wikidata::normalize(title);
    let mut ids: Vec<(i64, String)> = conn
        .prepare_cached(
            "SELECT DISTINCT g.id, g.name FROM names n JOIN games g ON g.id = n.id
             WHERE n.key = ?1 AND g.platform IN (SELECT value FROM json_each(?2))",
        )?
        .query_map(params![key, platforms], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    // A game's own name wins over another game's alternate name ("Streets
    // of Rage" over a hack that lists it as one); still exactly one or none.
    if ids.len() > 1
        && ids
            .iter()
            .any(|(_, name)| crate::wikidata::normalize(name) == key)
    {
        ids.retain(|(_, name)| crate::wikidata::normalize(name) == key);
    }
    let [(id, _)] = ids.as_slice() else {
        return Ok(None);
    };
    let g: [Option<String>; 13] = conn.query_row(
        "SELECT overview, developer, publisher, genres, release_date, release_year, max_players,
                esrb, rating, rating_count, wikipedia_url, video_url, cooperative
         FROM games WHERE id = ?1",
        [id],
        |r| {
            Ok([
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
                r.get(10)?,
                r.get(11)?,
                r.get(12)?,
            ])
        },
    )?;
    let [
        overview,
        dev,
        publ,
        genres,
        date,
        year,
        players,
        esrb,
        rating,
        rating_count,
        wiki,
        video,
        _coop,
    ] = g;
    let mut f = Found::default();
    let mut put = |k: &'static str, v: Option<String>| {
        if let Some(v) = v.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()) {
            f.fields.push((k, v));
        }
    };
    put("description", overview);
    put("developer", dev);
    put("publisher", publ);
    put(
        "genre",
        genres.map(|g| g.split(';').map(str::trim).collect::<Vec<_>>().join(", ")),
    );
    // "1991-08-13T00:00:00-07:00", or a year alone.
    match date.as_deref().map(|d| d.get(..10).unwrap_or(d)) {
        Some(d) if d.len() == 10 => {
            put("releaseyear", Some(d[..4].to_owned()));
            put("releasemonth", d[5..7].parse::<u32>().ok().map(|m| m.to_string()));
            put("releaseday", d[8..10].parse::<u32>().ok().map(|m| m.to_string()));
        }
        _ => put("releaseyear", year),
    }
    put("users", players);
    put("esrb", esrb.filter(|e| e != "Not Rated"));
    if let (Some(Ok(r)), Some(n)) = (
        rating.map(|r| r.parse::<f64>()),
        rating_count.filter(|n| n != "0"),
    ) {
        put("community_rating", Some(format!("{r:.1} / 5 ({n} votes)")));
    }
    put("wikipedia", wiki.as_deref().and_then(wikipedia_title));
    put("video", video);

    // Images: per kind, the game's own region first, then worldwide, then
    // untagged, then any.
    let want = regions_of(title);
    let mut imgs: Vec<(String, String, Option<String>)> = conn
        .prepare_cached("SELECT file, type, region FROM images WHERE id = ?1 ORDER BY rowid")?
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    let rank = |region: &Option<String>| match region.as_deref() {
        Some(r) if want.contains(&r) => 0,
        Some("World") => 1,
        None => 2,
        Some(_) => 3,
    };
    imgs.sort_by_key(|(_, _, region)| rank(region));
    for (kind, types) in [
        (
            ArtKind::Boxart,
            &["Box - Front", "Box - Front - Reconstructed"][..],
        ),
        (ArtKind::Box3d, &["Box - 3D"]),
        (ArtKind::Spine, &["Box - Spine"]),
        (
            ArtKind::Boxback,
            &["Box - Back", "Box - Back - Reconstructed"][..],
        ),
        (ArtKind::Title, &["Screenshot - Game Title"]),
        (ArtKind::Snap, &["Screenshot - Gameplay"]),
        (ArtKind::Hero, &["Fanart - Background"]),
        (ArtKind::Logo, &["Clear Logo"]),
        (ArtKind::Icon, &["Icon"]),
        (ArtKind::Boxfull, &["Box - Full"]),
        (ArtKind::Media, &["Cart - Front", "Disc", "Cart - 3D"]),
        (ArtKind::Marquee, &["Arcade - Marquee"]),
    ] {
        // Each type in order (a reconstructed box after the real ones),
        // each region-ranked.
        for t in types {
            for (file, _, region) in imgs.iter().filter(|(_, ty, _)| ty == t) {
                f.images.push(crate::enrich::ArtFound {
                    kind,
                    url: format!("{images_base}/{}", fetch::encode_segment(file)),
                    thumb: None,
                    label: region.clone(),
                });
            }
        }
    }
    Ok(Some(f))
}

/// The article title of an English Wikipedia URL, decoded
/// ("…/wiki/Pok%C3%A9mon_Red_and_Blue#x" -> "Pokémon Red and Blue"), so a
/// link made from it is encoded once.
pub(super) fn wikipedia_title(url: &str) -> Option<String> {
    let path = url.split("/wiki/").nth(1)?;
    let path = path.split(['#', '?']).next().unwrap_or(path);
    Some(fetch::decode_segment(path).replace('_', " "))
}

/// LaunchBox regions for a DAT name's region tag.
pub(super) fn regions_of(title: &str) -> Vec<&'static str> {
    let tags = title.to_ascii_lowercase();
    let mut out = Vec::new();
    for (tag, regions) in [
        ("usa", &["North America", "United States"][..]),
        ("canada", &["North America", "Canada"]),
        ("europe", &["Europe"]),
        ("uk", &["United Kingdom", "Europe"]),
        ("germany", &["Germany", "Europe"]),
        ("france", &["France", "Europe"]),
        ("spain", &["Spain", "Europe"]),
        ("italy", &["Italy", "Europe"]),
        ("australia", &["Australia", "Oceania"]),
        ("japan", &["Japan"]),
        ("korea", &["Korea"]),
        ("world", &["World"]),
    ] {
        if tags.contains(&format!("({tag}")) || tags.contains(&format!(", {tag}")) {
            out.extend_from_slice(regions);
        }
    }
    out
}
