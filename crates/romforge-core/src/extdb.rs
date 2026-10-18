//! Milestone 11: third-party game databases the user downloads from inside
//! romforge. Free and without an account, but published without a licence,
//! so romforge never ships their data: each user fetches them from the
//! source onto their own disk, and Enrich uses them to fill only what the
//! open sources (libretro, Wikidata) leave empty (house rule 10).
//!
//! - OpenVGDB (OpenEmu's database): matched by SHA1, so strict (house rule
//!   8). Kept as its own SQLite file, with an index added on the hashes.
//! - LaunchBox Games Database (`Metadata.zip`): no hashes, so matched by
//!   exact title (accents and punctuation aside) on the platforms named in
//!   `systems.rs`, and only when exactly one game fits. Read as a stream
//!   from the zip into a small SQLite index of the platforms romforge knows;
//!   its images are fetched one by one when Enrich needs them.

use std::collections::BTreeSet;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use quick_xml::XmlVersion;
use quick_xml::events::Event;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use crate::enrich::ArtKind;
use crate::{Error, fetch, systems};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtDb {
    OpenVgdb,
    LaunchBox,
}

impl ExtDb {
    pub const ALL: [ExtDb; 2] = [ExtDb::OpenVgdb, ExtDb::LaunchBox];

    pub fn key(self) -> &'static str {
        match self {
            ExtDb::OpenVgdb => "openvgdb",
            ExtDb::LaunchBox => "launchbox",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|d| d.key().eq_ignore_ascii_case(s.trim()))
    }

    pub fn name(self) -> &'static str {
        match self {
            ExtDb::OpenVgdb => "OpenVGDB",
            ExtDb::LaunchBox => "LaunchBox Games Database",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            ExtDb::OpenVgdb => {
                "OpenEmu's database of 52,000 ROMs by hash: descriptions, developer, publisher, genre, release date. \
                 Last updated 2021. About 9 MB."
            }
            ExtDb::LaunchBox => {
                "LaunchBox's community database of 190,000 games, updated daily: overviews, studios, genres, \
                 players, ratings, and box, 3D box, logo, fanart and screenshot images. About 110 MB to download."
            }
        }
    }

    pub fn homepage(self) -> &'static str {
        match self {
            ExtDb::OpenVgdb => "https://github.com/OpenVGDB/OpenVGDB",
            ExtDb::LaunchBox => "https://gamesdb.launchbox-app.com",
        }
    }

    fn file(self) -> &'static str {
        match self {
            ExtDb::OpenVgdb => "openvgdb.sqlite",
            ExtDb::LaunchBox => "launchbox.sqlite",
        }
    }
}

/// Where the databases are fetched from (tests point these elsewhere).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bases {
    pub openvgdb: String,
    pub launchbox: String,
    pub launchbox_images: String,
}

impl Default for Bases {
    fn default() -> Self {
        Bases {
            openvgdb: "https://github.com/OpenVGDB/OpenVGDB/releases/latest/download/openvgdb.zip".into(),
            launchbox: "https://gamesdb.launchbox-app.com/Metadata.zip".into(),
            launchbox_images: "https://images.launchbox-app.com".into(),
        }
    }
}

/// The folder for the downloaded databases: `databases/` next to the
/// library database.
pub fn dir_for(db_path: &Path) -> PathBuf {
    db_path.parent().unwrap_or(Path::new(".")).join("databases")
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ExtDbInfo {
    pub db: ExtDb,
    pub name: &'static str,
    pub about: &'static str,
    pub homepage: &'static str,
    /// When it was downloaded, if it was.
    pub downloaded_at: Option<String>,
    pub bytes: Option<u64>,
    /// Games (LaunchBox) or ROMs (OpenVGDB) in the local copy.
    pub entries: Option<i64>,
}

/// The local copy of a database, if downloaded, opened read-only.
pub fn open(db: ExtDb, dir: &Path) -> Option<Connection> {
    let path = dir.join(db.file());
    if !path.exists() {
        return None;
    }
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()
}

pub fn info(db: ExtDb, dir: &Path) -> ExtDbInfo {
    let path = dir.join(db.file());
    let conn = open(db, dir);
    let get = |key: &str| -> Option<String> {
        conn.as_ref()?
            .query_row("SELECT value FROM romforge_meta WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .ok()?
    };
    ExtDbInfo {
        db,
        name: db.name(),
        about: db.about(),
        homepage: db.homepage(),
        downloaded_at: get("downloaded_at"),
        bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
        entries: get("entries").and_then(|v| v.parse().ok()),
    }
}

pub fn list(dir: &Path) -> Vec<ExtDbInfo> {
    ExtDb::ALL.into_iter().map(|d| info(d, dir)).collect()
}

/// Delete the local copy.
pub fn remove(db: ExtDb, dir: &Path) -> Result<(), Error> {
    match std::fs::remove_file(dir.join(db.file())) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Download (or update) a database and build its local copy. The download
/// replaces the old copy only once the new one is complete.
pub fn download(
    db: ExtDb,
    dir: &Path,
    bases: &Bases,
    mut progress: impl FnMut(&str),
) -> Result<ExtDbInfo, Error> {
    std::fs::create_dir_all(dir)?;
    let zip = dir.join(format!("{}.zip", db.key()));
    let url = match db {
        ExtDb::OpenVgdb => &bases.openvgdb,
        ExtDb::LaunchBox => &bases.launchbox,
    };
    let mut last = 0;
    fetch::download_to(url, &zip, |done, total| {
        if done - last >= 8 << 20 || Some(done) == total {
            last = done;
            progress(&match total {
                Some(t) => format!("downloading {} MB of {} MB", done >> 20, t >> 20),
                None => format!("downloading {} MB", done >> 20),
            });
        }
    })?;
    let result = import(db, &zip, dir, url, progress);
    let _ = std::fs::remove_file(&zip);
    result
}

/// Build the local copy from a zip already on disk (a download, or one the
/// user fetched by hand). `source` is noted as where it came from.
pub fn import(
    db: ExtDb,
    zip: &Path,
    dir: &Path,
    source: &str,
    mut progress: impl FnMut(&str),
) -> Result<ExtDbInfo, Error> {
    std::fs::create_dir_all(dir)?;
    let part = dir.join(format!("{}.part", db.file()));
    let _ = std::fs::remove_file(&part);
    let built = match db {
        ExtDb::OpenVgdb => build_openvgdb(zip, &part),
        ExtDb::LaunchBox => build_launchbox(zip, &part, &mut progress),
    };
    let entries = match built {
        Ok(n) => n,
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
    };
    {
        let conn = Connection::open(&part)?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS romforge_meta (key TEXT PRIMARY KEY, value TEXT)")?;
        for (k, v) in [
            ("downloaded_at", now()),
            ("entries", entries.to_string()),
            ("source", source.to_owned()),
        ] {
            conn.execute(
                "INSERT OR REPLACE INTO romforge_meta (key, value) VALUES (?1, ?2)",
                params![k, v],
            )?;
        }
    }
    std::fs::rename(&part, dir.join(db.file()))?;
    Ok(info(db, dir))
}

fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    Connection::open_in_memory()
        .and_then(|c| {
            c.query_row(
                "SELECT strftime('%Y-%m-%dT%H:%M:%SZ', ?1, 'unixepoch')",
                [secs],
                |r| r.get(0),
            )
        })
        .unwrap_or_default()
}

/// OpenVGDB comes as one SQLite file in a zip: copy it out and index the
/// hashes.
fn build_openvgdb(zip: &Path, out: &Path) -> Result<i64, Error> {
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

/// LaunchBox's Metadata.xml, streamed out of the zip into a small index of
/// the platforms romforge knows.
fn build_launchbox(zip: &Path, out: &Path, progress: &mut impl FnMut(&str)) -> Result<i64, Error> {
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
                    put_name.execute(params![id, match_key(name), Option::<&str>::None])?;
                    games += 1;
                    if games % 20_000 == 0 {
                        progress(&format!("reading: {games} games"));
                    }
                }
                "GameAlternateName" => {
                    if let Some(n) = get("AlternateName") {
                        put_name.execute(params![id, match_key(n), get("Region")])?;
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

/// How titles are compared: Wikidata's normalization (no tags, no
/// punctuation, articles in place), with accents folded ("Pokémon" =
/// "Pokemon").
pub fn match_key(title: &str) -> String {
    crate::wikidata::normalize(&fold_accents(title))
}

pub fn fold_accents(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let r = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => "a",
            'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' => "A",
            'è' | 'é' | 'ê' | 'ë' | 'ē' => "e",
            'È' | 'É' | 'Ê' | 'Ë' | 'Ē' => "E",
            'ì' | 'í' | 'î' | 'ï' | 'ī' => "i",
            'Ì' | 'Í' | 'Î' | 'Ï' | 'Ī' => "I",
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => "o",
            'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' => "O",
            'ù' | 'ú' | 'û' | 'ü' | 'ū' => "u",
            'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ū' => "U",
            'ñ' => "n",
            'Ñ' => "N",
            'ç' => "c",
            'Ç' => "C",
            'ý' | 'ÿ' => "y",
            'Ý' => "Y",
            'ß' => "ss",
            'æ' => "ae",
            'Æ' => "AE",
            'œ' => "oe",
            'Œ' => "OE",
            _ => {
                out.push(c);
                continue;
            }
        };
        out.push_str(r);
    }
    out
}

/// What a database knows about one game, as `game_meta` fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    pub fields: Vec<(&'static str, String)>,
    /// LaunchBox only: (kind, image URL), best region first.
    pub images: Vec<(ArtKind, String)>,
}

/// OpenVGDB by any of the game's SHA1s.
pub fn openvgdb_lookup(conn: &Connection, sha1s: &[String]) -> Result<Option<Found>, Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT e.releaseDescription, e.releaseDeveloper, e.releasePublisher, e.releaseGenre, e.releaseDate
         FROM ROMs r CROSS JOIN RELEASES e ON e.romID = r.romID
         WHERE r.romHashSHA1 = ?1
         ORDER BY (e.releaseDescription IS NULL), e.releaseID LIMIT 1",
    )?;
    for sha1 in sha1s {
        let row: Option<[Option<String>; 5]> = stmt
            .query_row([sha1.to_ascii_uppercase()], |r| {
                Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?])
            })
            .optional()?;
        let Some([desc, dev, publ, genre, date]) = row else {
            continue;
        };
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

/// "Aug 13, 1991", "Aug 1991" or "1991".
fn parse_us_date(s: &str) -> Option<(i32, Option<u32>, Option<u32>)> {
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
    let key = match_key(title);
    let mut ids: Vec<(i64, String)> = conn
        .prepare_cached(
            "SELECT DISTINCT g.id, g.name FROM names n JOIN games g ON g.id = n.id
             WHERE n.key = ?1 AND g.platform IN (SELECT value FROM json_each(?2))",
        )?
        .query_map(params![key, platforms], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    // A game's own name wins over another game's alternate name ("Streets
    // of Rage" over a hack that lists it as one); still exactly one or none.
    if ids.len() > 1 && ids.iter().any(|(_, name)| match_key(name) == key) {
        ids.retain(|(_, name)| match_key(name) == key);
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
    put(
        "wikipedia",
        wiki.as_deref()
            .and_then(|u| u.split("/wiki/").nth(1))
            .map(|a| a.replace('_', " ")),
    );
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
        (ArtKind::Title, &["Screenshot - Game Title"]),
        (ArtKind::Snap, &["Screenshot - Gameplay"]),
        (ArtKind::Hero, &["Fanart - Background"]),
        (ArtKind::Logo, &["Clear Logo"]),
        (ArtKind::Icon, &["Icon"]),
    ] {
        let best = types.iter().find_map(|t| imgs.iter().find(|(_, ty, _)| ty == t));
        if let Some((file, _, _)) = best {
            f.images
                .push((kind, format!("{images_base}/{}", fetch::encode_segment(file))));
        }
    }
    Ok(Some(f))
}

/// LaunchBox regions for a DAT name's region tag.
fn regions_of(title: &str) -> Vec<&'static str> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_fold_accents_and_tags() {
        assert_eq!(
            match_key("Pokémon: Emerald Version"),
            match_key("Pokemon - Emerald Version (USA, Europe)")
        );
        assert_eq!(
            match_key("The Legend of Zelda: A Link to the Past"),
            match_key("Legend of Zelda, The - A Link to the Past (USA)")
        );
    }

    #[test]
    fn dates_and_regions() {
        assert_eq!(parse_us_date("Aug 13, 1991"), Some((1991, Some(8), Some(13))));
        assert_eq!(parse_us_date("1994"), Some((1994, None, None)));
        assert_eq!(parse_us_date("soon"), None);
        assert_eq!(
            regions_of("Pokemon - Emerald Version (USA, Europe)"),
            ["North America", "United States", "Europe"]
        );
        assert!(regions_of("Tetris").is_empty());
    }
}
