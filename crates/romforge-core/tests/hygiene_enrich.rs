//! Enrich and Search hygiene against a local stand-in server: credentials
//! never kept or shown, players kept as `users`, the list's fingerprint, and
//! Search on any row of the list.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use romforge_core::artsources::{self, Bases};
use romforge_core::enrich::{ArtKind, EnrichOptions};
use romforge_core::search::Scraper;
use romforge_core::settings::{Keys, Settings};
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake";

type Handler = fn(&str, &str) -> Option<Vec<u8>>;

/// Serves `answer(base, target)` (404 for `None`) on 127.0.0.1; returns the
/// base URL and every request's target (path and query).
fn serve(answer: Handler) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let (seen, host) = (log.clone(), base.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            let _ = reader.read_line(&mut first);
            let mut h = String::new();
            while reader.read_line(&mut h).unwrap_or(0) > 2 {
                h.clear();
            }
            let target = first.split_whitespace().nth(1).unwrap_or("/").to_owned();
            seen.lock().unwrap().push(target.clone());
            let (status, body) = match answer(&host, &target) {
                Some(b) => ("200 OK", b),
                None => ("404 Not Found", Vec::new()),
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(&body);
        }
    });
    (base, log)
}

fn options(base: &str, cache: &Path) -> EnrichOptions {
    let mut opts = EnrichOptions::new(cache);
    opts.metadata_base = format!("{base}/none");
    opts.dats_base = format!("{base}/dat");
    opts.thumbnails_base = format!("{base}/none");
    opts.wikidata = None;
    opts.sources = Bases {
        screenscraper: format!("{base}/ss"),
        steamgriddb: format!("{base}/sgdb"),
        igdb_token: format!("{base}/igdb/token"),
        igdb: format!("{base}/igdb"),
        igdb_images: format!("{base}/igimg"),
        adb: format!("{base}/adb"),
        gametdb: format!("{base}/gametdb"),
    };
    opts
}

/// ScreenScraper's media URLs carry the credentials, as the real one's do;
/// a picture is served only with them.
fn screenscraper(base: &str, target: &str) -> Option<Vec<u8>> {
    let path = target.split('?').next().unwrap_or("");
    match path {
        "/ss/jeuInfos.php" => Some(
            format!(
                r#"{{"response":{{"jeu":{{"medias":[{{"type":"box-2D","region":"us",
                "url":"{base}/media/box.png?devid=dev&devpassword=devpw&softname=romforge&ssid=me&sspassword=mypw&media=box-2D"}}]}}}}}}"#
            )
            .into_bytes(),
        ),
        "/media/box.png" if target.contains("devpassword=devpw") && target.contains("sspassword=mypw") => {
            Some(PNG.to_vec())
        }
        _ => None,
    }
}

#[test]
fn screenscraper_credentials_are_never_kept_or_shown() {
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(fixtures().join("roms/gb"), Scope::All).unwrap();
    let s = Settings {
        keys: Keys {
            screenscraper_dev_id: Some("dev".into()),
            screenscraper_dev_password: Some("devpw".into()),
            screenscraper_user: Some("me".into()),
            screenscraper_password: Some("mypw".into()),
            ..Keys::default()
        },
        ..Settings::default()
    };
    lib.save_settings(&s).unwrap();
    let (base, log) = serve(screenscraper);
    let cache = tempfile::tempdir().unwrap();
    let mut opts = options(&base, cache.path());
    opts.system = Some("Game Boy".into());
    opts.title = Some("Test GB".into());
    let report = lib.enrich(&opts, |_| {}).unwrap();
    assert_eq!(report.art_saved, 1, "downloaded with the credentials: {report:?}");

    let row = lib
        .library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.title == "Test GB")
        .unwrap();
    let choices = lib.art_choices(&opts, &row.id, ArtKind::Boxart).unwrap();
    let ss = choices
        .choices
        .iter()
        .find(|c| c.source == "screenscraper")
        .unwrap_or_else(|| panic!("{choices:?} {:?}", log.lock().unwrap()));
    assert!(
        !ss.url.contains("password") && !ss.url.contains("devid"),
        "{}",
        ss.url
    );
    assert!(
        ss.url.contains("media=box-2D"),
        "other parameters stay: {}",
        ss.url
    );
    assert!(ss.current, "the kept URL is the same, without credentials");

    lib.choose_art(&opts, &row.id, ArtKind::Boxart, &ss.url, "screenscraper")
        .unwrap();
    let e = lib
        .choose_art(
            &opts,
            &row.id,
            ArtKind::Boxart,
            &format!("{base}/media/gone.png?devpassword=devpw"),
            "screenscraper",
        )
        .unwrap_err();
    assert!(!e.to_string().contains("devpw"), "{e}");
}

#[test]
fn credentials_go_back_only_to_screenscraper() {
    let raw = "https://neoclone.screenscraper.fr/api2/mediaJeu.php?devid=a&devpassword=b&softname=romforge&ssid=c&sspassword=d&media=box";
    let clean = artsources::without_credentials(raw);
    assert_eq!(
        clean,
        "https://neoclone.screenscraper.fr/api2/mediaJeu.php?softname=romforge&media=box"
    );
    let keys = Keys {
        screenscraper_dev_id: Some("a".into()),
        screenscraper_password: Some("p w".into()),
        ..Keys::default()
    };
    let bases = Bases::default();
    let back = artsources::download_url("screenscraper", &clean, &keys, &bases);
    assert!(back.ends_with("&devid=a&sspassword=p%20w"), "{back}");
    let elsewhere = "https://evil.example/x.png?y=1";
    assert_eq!(
        artsources::download_url("screenscraper", elsewhere, &keys, &bases),
        elsewhere
    );
    assert_eq!(artsources::download_url("igdb", &clean, &keys, &bases), clean);
}

fn dos(_: &str, target: &str) -> Option<Vec<u8>> {
    (target == "/dat/DOS.dat").then(|| {
        let crc = format!("{:08x}", crc32fast::hash(&[3u8; 1500]));
        format!(
            "clrmamepro (\n\tname \"DOS\"\n)\ngame (\n\tname \"Commander Keen\"\n\tusers \"2\"\n\tgenre \"Platform\"\n\trom ( name \"KEEN1.EXE\" size 1500 crc {crc} )\n)\n"
        )
        .into_bytes()
    })
}

#[test]
fn players_from_libretros_dats_are_kept_as_users() {
    let tmp = tempfile::tempdir().unwrap();
    let keen = tmp.path().join("incoming").join("KEEN");
    std::fs::create_dir_all(&keen).unwrap();
    std::fs::write(keen.join("KEEN1.EXE"), [3u8; 1500]).unwrap();
    let mut lib = Library::open(tmp.path().join("lib.db")).unwrap();
    lib.scan(tmp.path().join("incoming"), Scope::All).unwrap();
    let row = lib
        .library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.title == "KEEN1.EXE")
        .unwrap();
    lib.identify_as(&row.id, "DOS", "My Keen").unwrap();
    let (base, _) = serve(dos);
    let report = lib
        .enrich(&options(&base, &tmp.path().join("cache")), |_| {})
        .unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let row = lib
        .library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.title == "My Keen")
        .unwrap();
    let d = lib.game_detail(&row.id).unwrap();
    assert_eq!(
        d.fields.get("users").map(String::as_str),
        Some("2"),
        "{:?}",
        d.fields
    );
    assert_eq!(d.fields.get("maxusers"), None);
}

#[test]
fn a_nested_scan_root_changes_the_fingerprint() {
    let tmp = tempfile::tempdir().unwrap();
    let inner = tmp.path().join("roms").join("dos");
    std::fs::create_dir_all(&inner).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.scan(tmp.path().join("roms"), Scope::All).unwrap();
    let before = lib.view_generation().unwrap();
    lib.scan(&inner, Scope::All).unwrap();
    assert_ne!(lib.view_generation().unwrap(), before);
}

#[test]
fn search_takes_any_row_of_the_list() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("patches");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Lone Hack.ips"), b"PATCH\x00\x00\x10\x00\x01\x41EOF").unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.scan(&dir, Scope::All).unwrap();
    let rows = lib.library_view().unwrap().rows;
    let patch = rows
        .iter()
        .find(|r| r.status == RowStatus::Patch)
        .unwrap_or_else(|| panic!("{rows:#?}"));
    // A row only the list has is found, not "no library row".
    lib.search_find(
        &patch.id,
        Scraper::Dat,
        Some("Lone Hack"),
        None,
        "http://127.0.0.1:1",
    )
    .unwrap();
    assert!(
        lib.search_queue(None)
            .unwrap()
            .iter()
            .all(|q| q.row_id != patch.id)
    );
    let e = lib.identify_as(&patch.id, "Game Boy", "Lone Hack").unwrap_err();
    assert!(e.to_string().contains("patch file"), "{e}");
}
