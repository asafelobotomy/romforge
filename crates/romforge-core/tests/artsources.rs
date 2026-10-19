//! Keyed art sources (ScreenScraper, SteamGridDB, IGDB) against one local
//! stand-in server: house rule 10's order, and only what's missing is filled.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use romforge_core::artsources::Bases;
use romforge_core::enrich::{ArtKind, EnrichOptions};
use romforge_core::settings::Settings;
use romforge_core::{Library, Scope};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake";
const JPG: &[u8] = b"\xff\xd8\xff\xe0fake";

/// (path with query, headers) of every request.
type Log = Arc<Mutex<Vec<(String, HashMap<String, String>)>>>;

fn serve() -> (String, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let log: Log = Arc::default();
    let seen = log.clone();
    let host = base.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let mut headers = HashMap::new();
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                    break;
                }
                if let Some((k, v)) = h.trim_end().split_once(": ") {
                    headers.insert(k.to_ascii_lowercase(), v.to_owned());
                }
            }
            let len: usize = headers
                .get("content-length")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let mut body = vec![0u8; len];
            reader.read_exact(&mut body).unwrap();
            let target = first.split_whitespace().nth(1).unwrap_or("").to_owned();
            seen.lock().unwrap().push((target.clone(), headers));
            let path = target.split('?').next().unwrap_or("");
            let (ctype, data): (&str, Vec<u8>) = match path {
                "/ss/jeuInfos.php" => (
                    "application/json",
                    format!(
                        r#"{{"response":{{"jeu":{{"medias":[
                            {{"type":"box-2D","region":"jp","url":"{host}/img/box-jp.png"}},
                            {{"type":"box-2D","region":"us","url":"{host}/img/box-us.png"}},
                            {{"type":"box-3D","region":"us","url":"{host}/img/box3d.png"}},
                            {{"type":"wheel","region":"wor","url":"{host}/img/wheel.png"}},
                            {{"type":"manuel","region":"us","url":"{host}/img/manual.pdf"}}]}}}}}}"#
                    )
                    .into_bytes(),
                ),
                "/sgdb/search/autocomplete/Test%20GB" => (
                    "application/json",
                    br#"{"success":true,"data":[{"id":70,"name":"Test GB Color"},{"id":7,"name":"Test GB"}]}"#.to_vec(),
                ),
                // Two games of that very title: SteamGridDB can't say which.
                "/sgdb/search/autocomplete/MD5%20Only" => (
                    "application/json",
                    br#"{"success":true,"data":[{"id":80,"name":"MD5 Only"},{"id":81,"name":"MD5 Only"}]}"#.to_vec(),
                ),
                // Two heroes for Test GB, each with a thumbnail.
                "/sgdb/heroes/game/7" => (
                    "application/json",
                    format!(
                        r#"{{"success":true,"data":[
                            {{"url":"{host}/img/hero-a.jpg","thumb":"{host}/img/hero-a-thumb.jpg","width":1920,"height":620,"style":"alternate"}},
                            {{"url":"{host}/img/hero-b.jpg","thumb":"{host}/img/hero-b-thumb.jpg","width":3840,"height":1240,"style":"blurred"}}]}}"#
                    )
                    .into_bytes(),
                ),
                p if p.starts_with("/sgdb/") && (p.contains("/game/7") || p.contains("/game/8")) => (
                    "application/json",
                    format!(r#"{{"success":true,"data":[{{"url":"{host}/img/sgdb{}.jpg"}}]}}"#, p.len()).into_bytes(),
                ),
                "/igdb/token" => ("application/json", br#"{"access_token":"tok","expires_in":100}"#.to_vec()),
                "/igdb/games" => (
                    "application/json",
                    br#"[{"name":"Test GB","cover":{"image_id":"c1"},"screenshots":[{"image_id":"s1"}]}]"#.to_vec(),
                ),
                p if p.starts_with("/img/") && p.ends_with(".png") => ("image/png", PNG.to_vec()),
                p if (p.starts_with("/img/") || p.starts_with("/igimg/")) && p.ends_with(".jpg") => {
                    ("image/jpeg", JPG.to_vec())
                }
                _ => ("text/plain", Vec::new()),
            };
            let status = if data.is_empty() {
                "404 Not Found"
            } else {
                "200 OK"
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                data.len()
            );
            let _ = stream.write_all(&data);
        }
    });
    (base, log)
}

#[test]
fn keyed_sources_fill_only_what_is_missing_in_order() {
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(fixtures().join("roms/gb"), Scope::All).unwrap();
    let mut s = Settings::default();
    s.keys.screenscraper_dev_id = Some("dev".into());
    s.keys.screenscraper_dev_password = Some("devpw".into());
    s.keys.steamgriddb_key = Some("sgdbkey".into());
    s.keys.igdb_client_id = Some("cid".into());
    s.keys.igdb_client_secret = Some("csecret".into());
    lib.save_settings(&s).unwrap();

    let (base, log) = serve();
    let cache = tempfile::tempdir().unwrap();
    let mut opts = EnrichOptions::new(cache.path());
    opts.system = Some("Game Boy".into());
    opts.art = ArtKind::ALL.to_vec();
    opts.metadata_base = format!("{base}/none");
    opts.thumbnails_base = format!("{base}/none");
    opts.wikidata = None;
    opts.sources = Bases {
        screenscraper: format!("{base}/ss"),
        steamgriddb: format!("{base}/sgdb"),
        igdb_token: format!("{base}/igdb/token"),
        igdb: format!("{base}/igdb"),
        igdb_images: format!("{base}/igimg"),
    };
    let report = lib.enrich(&opts, |_| {}).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);

    let info = lib.game_info("Test GB").unwrap();
    let g = info.iter().find(|g| g.title == "Test GB").unwrap();
    let file = |k: ArtKind| {
        g.art
            .get(&k)
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
    };
    // ScreenScraper (tier 2) first: box (US over Japan), 3D box, wheel logo.
    assert_eq!(file(ArtKind::Boxart).as_deref(), Some("Test GB.png"));
    assert!(g.art.contains_key(&ArtKind::Box3d) && g.art.contains_key(&ArtKind::Logo));
    // SteamGridDB then fills hero and icon (JPEGs keep their type).
    assert_eq!(file(ArtKind::Hero).as_deref(), Some("Test GB.jpg"));
    assert!(g.art.contains_key(&ArtKind::Icon));
    // IGDB last: only the screenshot was still missing.
    assert!(g.art.contains_key(&ArtKind::Snap));
    assert!(!g.art.contains_key(&ArtKind::Title), "nobody had a title screen");

    let requests = log.lock().unwrap().clone();
    let ss = requests
        .iter()
        .find(|(p, _)| p.starts_with("/ss/jeuInfos.php"))
        .unwrap();
    assert!(
        ss.0.contains("devid=dev") && ss.0.contains("sha1=") && ss.0.contains("systemeid=9"),
        "{}",
        ss.0
    );
    assert!(
        !requests
            .iter()
            .any(|(p, _)| p.contains("/game/80") || p.contains("/game/81")),
        "an ambiguous title gets nothing"
    );
    let sgdb = requests.iter().find(|(p, _)| p.starts_with("/sgdb/")).unwrap();
    assert_eq!(
        sgdb.1.get("authorization").map(String::as_str),
        Some("Bearer sgdbkey")
    );
    let igdb = requests.iter().find(|(p, _)| p == "/igdb/games").unwrap();
    assert_eq!(igdb.1.get("client-id").map(String::as_str), Some("cid"));
    assert_eq!(
        igdb.1.get("authorization").map(String::as_str),
        Some("Bearer tok")
    );
    assert!(
        !requests.iter().any(|(p, _)| p.contains("/grids/")),
        "SteamGridDB grids are Steam capsules, never box art"
    );
    let igdb_img: Vec<&String> = requests
        .iter()
        .map(|(p, _)| p)
        .filter(|p| p.starts_with("/igimg/"))
        .collect();
    assert!(
        igdb_img
            .iter()
            .all(|p| p.contains("/t_screenshot_huge/") || p.contains("/t_cover_big_2x/")),
        "IGDB at its full sizes: {igdb_img:?}"
    );
    assert!(
        !requests.iter().any(|(p, _)| p.contains("box-jp")),
        "the US box was preferred, the Japanese one never downloaded"
    );

    // A second run asks no source again for the games already looked up.
    let before = requests.len();
    lib.enrich(&opts, |_| {}).unwrap();
    let again: Vec<String> = log.lock().unwrap()[before..]
        .iter()
        .map(|(p, _)| p.clone())
        .collect();
    assert!(again.iter().all(|p| p.starts_with("/none")), "{again:?}");

    // The fanart picker offers both heroes, previewed by their thumbnails;
    // the first is the one in use.
    let id = lib
        .library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.title == "Test GB")
        .unwrap()
        .id;
    let c = lib.art_choices(&opts, &id, ArtKind::Hero).unwrap();
    let shown: Vec<(&str, Option<&str>, bool)> = c
        .choices
        .iter()
        .map(|x| (x.source.as_str(), x.label.as_deref(), x.current))
        .collect();
    assert_eq!(
        shown,
        [
            ("steamgriddb", Some("1920×620"), true),
            ("steamgriddb", Some("blurred · 3840×1240"), false)
        ],
        "{:?}",
        c.errors
    );
    assert!(c.choices.iter().all(|x| Path::new(&x.preview).exists()));
    let thumbs = log
        .lock()
        .unwrap()
        .iter()
        .filter(|(p, _)| p.ends_with("-thumb.jpg"))
        .count();
    assert_eq!(thumbs, 2, "previews from the thumbnails");

    // Picking the second keeps it, even through a refresh.
    lib.choose_art(&opts, &id, ArtKind::Hero, &c.choices[1].url, "steamgriddb")
        .unwrap();
    let mut refresh = opts.clone();
    refresh.refresh = true;
    lib.enrich(&refresh, |_| {}).unwrap();
    let c = lib.art_choices(&opts, &id, ArtKind::Hero).unwrap();
    assert!(c.choices[1].current && !c.choices[0].current, "{c:?}");
    let previews = log
        .lock()
        .unwrap()
        .iter()
        .filter(|(p, _)| p.ends_with("-thumb.jpg"))
        .count();
    assert_eq!(previews, 2, "previews kept from the first look");

    // The list's picture views get the files: box art, 3D box, fanart, logo.
    let row = lib
        .library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.title == "Test GB")
        .unwrap();
    for k in ["boxart", "box3d", "hero", "logo"] {
        let p = row
            .pictures
            .get(k)
            .unwrap_or_else(|| panic!("no {k}: {:?}", row.pictures));
        assert!(Path::new(p).exists(), "{p}");
    }
    assert!(!row.pictures.contains_key("snap"), "only what the views show");
}
