//! Milestone 11: databases the user downloads (OpenVGDB, LaunchBox), from a
//! local stand-in server with tiny hand-made copies. Their real data never
//! goes in this repository.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use romforge_core::enrich::{ArtKind, EnrichOptions};
use romforge_core::extdb::{self, Bases, ExtDb};
use romforge_core::{Library, Scope};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake";

/// A one-ROM OpenVGDB: "Test GB" by the fixture's SHA1.
fn openvgdb_zip(dir: &Path) -> Vec<u8> {
    let db = dir.join("openvgdb.sqlite");
    let c = rusqlite::Connection::open(&db).unwrap();
    c.execute_batch(
        "CREATE TABLE ROMs (romID INTEGER PRIMARY KEY, romHashSHA1 TEXT, romSize INTEGER);
         CREATE TABLE RELEASES (releaseID INTEGER PRIMARY KEY, romID INTEGER, releaseDescription TEXT,
           releaseDeveloper TEXT, releasePublisher TEXT, releaseGenre TEXT, releaseDate TEXT);
         INSERT INTO ROMs VALUES (1, '845D41D57C6F24CCB31CE71ACD35CC74638ACD19', 1024);
         -- The same SHA1 at another size is another ROM: never this one.
         INSERT INTO ROMs VALUES (2, '845D41D57C6F24CCB31CE71ACD35CC74638ACD19', 2048);
         INSERT INTO RELEASES VALUES (0, 2, 'Not this one.', 'Wrong Studio', NULL, NULL, NULL);
         INSERT INTO RELEASES VALUES (1, 1, 'A test game, from OpenVGDB.', 'Hash Studio', NULL, 'Action,Platformer', 'Aug 13, 1991');",
    )
    .unwrap();
    drop(c);
    zip_of(&[("openvgdb.sqlite", std::fs::read(&db).unwrap())])
}

/// A LaunchBox Metadata.zip: "Test GB" on Game Boy (and the same name on
/// another platform, which must not count; and a hack that has it as an
/// alternate name), with images.
fn launchbox_zip() -> Vec<u8> {
    let xml = r#"<?xml version="1.0" standalone="yes"?>
<LaunchBox>
  <Game><Name>Test GB</Name><DatabaseID>7</DatabaseID><Platform>Nintendo Game Boy</Platform>
    <Overview>From LaunchBox &amp; friends.</Overview><Developer>Box Studio</Developer>
    <Publisher>Box Publishing</Publisher><Genres>Platform; Action</Genres>
    <ReleaseDate>1990-04-21T00:00:00-07:00</ReleaseDate><MaxPlayers>2</MaxPlayers>
    <CommunityRating>3.8055</CommunityRating><CommunityRatingCount>18</CommunityRatingCount>
    <WikipediaURL>https://en.wikipedia.org/wiki/Test_GB</WikipediaURL>
    <VideoURL>https://www.youtube.com/watch?v=x</VideoURL></Game>
  <Game><Name>Test GB</Name><DatabaseID>8</DatabaseID><Platform>Arcade</Platform></Game>
  <Game><Name>Test GB Plus</Name><DatabaseID>10</DatabaseID><Platform>Nintendo Game Boy</Platform><Overview>A hack.</Overview></Game>
  <Game><Name>Another One</Name><DatabaseID>9</DatabaseID><Platform>Microsoft Xbox Series X/S</Platform></Game>
  <GameAlternateName><AlternateName>Tesuto GB</AlternateName><DatabaseID>7</DatabaseID><Region>Japan</Region></GameAlternateName>
  <GameAlternateName><AlternateName>Test GB</AlternateName><DatabaseID>10</DatabaseID></GameAlternateName>
  <GameImage><DatabaseID>7</DatabaseID><FileName>jp-box.png</FileName><Type>Box - Front</Type><Region>Japan</Region><CRC32>1</CRC32></GameImage>
  <GameImage><DatabaseID>7</DatabaseID><FileName>logo.png</FileName><Type>Clear Logo</Type><CRC32>2</CRC32></GameImage>
  <GameImage><DatabaseID>7</DatabaseID><FileName>fanart.png</FileName><Type>Fanart - Background</Type><CRC32>3</CRC32></GameImage>
  <GameImage><DatabaseID>8</DatabaseID><FileName>arcade.png</FileName><Type>Clear Logo</Type><CRC32>4</CRC32></GameImage>
</LaunchBox>"#;
    zip_of(&[
        ("Metadata.xml", xml.as_bytes().to_vec()),
        ("Platforms.xml", b"<LaunchBox/>".to_vec()),
    ])
}

fn zip_of(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in files {
        z.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap().into_inner()
}

/// Serves the two zips and PNGs; records every path asked for.
fn serve(ovgdb: Vec<u8>, lb: Vec<u8>) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = log.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                    break;
                }
            }
            let path = first.split_whitespace().nth(1).unwrap_or("").to_owned();
            seen.lock().unwrap().push(path.clone());
            let body: &[u8] = match path.as_str() {
                "/openvgdb.zip" => &ovgdb,
                "/Metadata.zip" => &lb,
                p if p.starts_with("/img/") && p.ends_with(".png") => PNG,
                _ => b"",
            };
            let status = if body.is_empty() {
                "404 Not Found"
            } else {
                "200 OK"
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(body);
            let _ = stream.flush();
            let _ = stream.read(&mut [0u8; 1]);
        }
    });
    (base, log)
}

#[test]
fn download_then_fill_only_what_is_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let (base, log) = serve(openvgdb_zip(tmp.path()), launchbox_zip());
    let bases = Bases {
        openvgdb: format!("{base}/openvgdb.zip"),
        launchbox: format!("{base}/Metadata.zip"),
        launchbox_images: format!("{base}/img"),
    };
    let dir = tmp.path().join("databases");
    assert!(extdb::list(&dir).iter().all(|i| i.downloaded_at.is_none()));
    let o = extdb::download(ExtDb::OpenVgdb, &dir, &bases, |_| {}).unwrap();
    assert_eq!(o.entries, Some(2));
    let l = extdb::download(ExtDb::LaunchBox, &dir, &bases, |_| {}).unwrap();
    assert_eq!(l.entries, Some(3), "only platforms romforge knows are kept");
    assert!(l.downloaded_at.is_some() && !dir.join("launchbox.zip").exists());

    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(fixtures().join("roms/gb"), Scope::All).unwrap();
    let mut opts = EnrichOptions::new(tmp.path().join("cache"));
    opts.art = vec![ArtKind::Boxart, ArtKind::Logo, ArtKind::Hero, ArtKind::Icon];
    opts.metadata_base = format!("{base}/none");
    opts.thumbnails_base = format!("{base}/none");
    opts.wikidata = None;
    opts.extdb = Some(dir.clone());
    opts.launchbox_images = format!("{base}/img");
    let report = lib.enrich(&opts, |_| {}).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(report.extdb_games >= 1);

    let v = lib.library_view().unwrap();
    let id = &v.rows.iter().find(|r| r.title == "Test GB").unwrap().id;
    let d = lib.game_detail(id).unwrap();
    let f = |k: &str| d.fields.get(k).map(String::as_str);
    // OpenVGDB (by hash) first; LaunchBox fills what it lacked.
    assert_eq!(f("description"), Some("A test game, from OpenVGDB."));
    assert_eq!(
        d.field_sources.get("description").map(String::as_str),
        Some("openvgdb")
    );
    assert_eq!(f("developer"), Some("Hash Studio"));
    assert_eq!(f("publisher"), Some("Box Publishing"));
    assert_eq!(f("genre"), Some("Action, Platformer"));
    assert_eq!(
        (f("releaseyear"), f("releasemonth"), f("releaseday")),
        (Some("1991"), Some("8"), Some("13"))
    );
    assert_eq!(f("users"), Some("2"));
    assert_eq!(f("community_rating"), Some("3.8 / 5 (18 votes)"));
    assert_eq!(f("wikipedia"), Some("Test GB"));
    assert!(d.links.iter().any(|l| l.label == "Video" && l.direct));
    // Images from LaunchBox, the Game Boy game's (not the arcade one's).
    assert_eq!(d.art_sources.get("logo").map(String::as_str), Some("launchbox"));
    assert!(d.art.contains_key("boxart") && d.art.contains_key("hero"));
    assert!(!d.art.contains_key("icon"), "LaunchBox had no icon");
    let asked = log.lock().unwrap().clone();
    assert!(!asked.iter().any(|p| p.contains("arcade")), "{asked:?}");

    // Removing it leaves what was filled; listing shows it gone.
    extdb::remove(ExtDb::LaunchBox, &dir).unwrap();
    assert!(extdb::info(ExtDb::LaunchBox, &dir).downloaded_at.is_none());

    // A refresh asks libretro again, which has nothing: that never wipes
    // the box art LaunchBox gave.
    opts.refresh = true;
    lib.enrich(&opts, |_| {}).unwrap();
    let d = lib.game_detail(id).unwrap();
    assert!(d.art.contains_key("boxart"), "{:?}", d.art);
    assert_eq!(d.art_sources.get("boxart").map(String::as_str), Some("launchbox"));
}

#[test]
fn launchbox_needs_exactly_one_game() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = tmp.path().join("Metadata.zip");
    std::fs::write(&zip, launchbox_zip()).unwrap();
    let dir = tmp.path().join("databases");
    extdb::import(ExtDb::LaunchBox, &zip, &dir, "by hand", |_| {}).unwrap();
    let db = extdb::open(ExtDb::LaunchBox, &dir).unwrap();
    let gb = romforge_core::systems::by_key("Game Boy").unwrap();
    let found = extdb::launchbox_lookup(&db, gb, "Tesuto GB (Japan)", "http://img")
        .unwrap()
        .unwrap();
    assert!(
        found
            .images
            .iter()
            .any(|(k, u)| *k == ArtKind::Boxart && u.ends_with("jp-box.png")),
        "alternate names match too"
    );
    let own = extdb::launchbox_lookup(&db, gb, "Test GB (USA)", "http://img")
        .unwrap()
        .unwrap();
    assert!(
        own.fields
            .iter()
            .any(|(k, v)| *k == "description" && v.starts_with("From LaunchBox")),
        "its own name wins over the hack's alternate name: {:?}",
        own.fields
    );
    assert!(
        extdb::launchbox_lookup(&db, gb, "Nothing Like It", "http://img")
            .unwrap()
            .is_none()
    );
    let nes = romforge_core::systems::by_key("NES").unwrap();
    assert!(
        extdb::launchbox_lookup(&db, nes, "Test GB", "http://img")
            .unwrap()
            .is_none(),
        "platform must fit"
    );
}
