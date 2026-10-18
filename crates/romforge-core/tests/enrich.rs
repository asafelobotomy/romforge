//! Milestone 3 against a local stand-in for libretro's servers.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use romforge_core::enrich::{ArtKind, EnrichOptions};
use romforge_core::{Library, Scope};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// Serves `files` (path -> body) over HTTP on 127.0.0.1, 404 for the rest.
/// Returns the base URL and the list of requested paths.
fn serve(files: HashMap<String, Vec<u8>>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = log.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                    break;
                }
            }
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
            seen.lock().unwrap().push(path.clone());
            let (status, body) = match files.get(&path) {
                Some(b) => ("200 OK", b.clone()),
                None => ("404 Not Found", b"not found".to_vec()),
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

#[test]
fn metadata_and_art_for_games_you_have() {
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(fixtures().join("roms/gb"), Scope::All).unwrap();

    let system = "Nintendo%20-%20Game%20Boy";
    let mut files = HashMap::new();
    files.insert(
        format!("/meta/genre/{system}.dat"),
        b"clrmamepro ( name \"x\" )\ngame (\n\tcomment \"Test GB\"\n\tgenre \"Puzzle\"\n\trom ( crc 00000000 )\n)\n".to_vec(),
    );
    files.insert(
        format!("/meta/developer/{system}.dat"),
        b"game ( name \"Test GB\" developer \"Nobody\" )\n".to_vec(),
    );
    files.insert(
        format!("/thumbs/{system}/Named_Boxarts/Test%20GB.png"),
        b"\x89PNG\r\n\x1a\nfake".to_vec(),
    );
    let (base, log) = serve(files);
    let cache = tempfile::tempdir().unwrap();
    let mut opts = EnrichOptions::new(cache.path());
    opts.metadata_base = format!("{base}/meta");
    opts.thumbnails_base = format!("{base}/thumbs");
    opts.art = vec![ArtKind::Boxart, ArtKind::Snap];
    opts.wikidata = None;

    let report = lib.enrich(&opts, |_| {}).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.systems, 1);
    assert!(report.games >= 3, "{report:?}");
    assert_eq!(
        report.games_with_metadata,
        1,
        "{report:?} {:?}",
        log.lock().unwrap()
    );
    assert_eq!(report.art_saved, 1);
    assert_eq!(report.art_missing, report.games * 2 - 1);

    let info = lib.game_info("Test GB").unwrap();
    let g = &info[0];
    assert_eq!(g.title, "Test GB");
    assert_eq!(g.fields.get("genre").map(String::as_str), Some("Puzzle"));
    assert_eq!(g.fields.get("developer").map(String::as_str), Some("Nobody"));
    let png = &g.art[&ArtKind::Boxart];
    assert!(png.starts_with(cache.path()));
    assert_eq!(std::fs::read(png).unwrap(), b"\x89PNG\r\n\x1a\nfake");

    // A second run asks for nothing: metadata was checked, art is cached or
    // known missing.
    let before = log.lock().unwrap().len();
    let again = lib.enrich(&opts, |_| {}).unwrap();
    assert_eq!(log.lock().unwrap().len(), before, "no requests the second time");
    assert_eq!(again.art_cached, 1, "the one image on disk");
    assert_eq!(again.art_missing, report.games * 2 - 1, "the rest known missing");
    // --refresh fetches again.
    opts.refresh = true;
    lib.enrich(&opts, |_| {}).unwrap();
    assert!(log.lock().unwrap().len() > before);
}

/// libretro found the game by CRC under another name (No-Intro renamed it
/// since): its thumbnails go by libretro's name.
#[test]
fn thumbnails_by_libretros_own_name() {
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(fixtures().join("roms/gb"), Scope::All).unwrap();

    let system = "Nintendo%20-%20Game%20Boy";
    let mut files = HashMap::new();
    files.insert(
        format!("/meta/genre/{system}.dat"),
        b"game (\n\tcomment \"MD5 Only (World)\"\n\tgenre \"Racing\"\n\trom ( crc 849D60AE )\n)\n".to_vec(),
    );
    files.insert(
        format!("/thumbs/{system}/Named_Boxarts/MD5%20Only%20(World).png"),
        b"\x89PNG\r\n\x1a\nfake".to_vec(),
    );
    let (base, _log) = serve(files);
    let cache = tempfile::tempdir().unwrap();
    let mut opts = EnrichOptions::new(cache.path());
    opts.metadata_base = format!("{base}/meta");
    opts.thumbnails_base = format!("{base}/thumbs");
    opts.art = vec![ArtKind::Boxart];
    opts.wikidata = None;
    let report = lib.enrich(&opts, |_| {}).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);

    let g = &lib.game_info("MD5 Only").unwrap()[0];
    assert_eq!(g.fields.get("genre").map(String::as_str), Some("Racing"));
    assert!(g.art.contains_key(&ArtKind::Boxart), "{g:?}");
}

/// A translation shows its base game's metadata and art where it has
/// none of its own; Enrich fetches the base too.
#[test]
fn derivatives_show_their_base() {
    use romforge_core::view::{Derived, Relation};
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("roms");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("Test GB (T-Eng).gb"), vec![9u8; 2048]).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(&dir, Scope::All).unwrap();
    let row = lib.search_queue(None).unwrap()[0].row_id.clone();
    let d = Derived {
        relation: Relation::Translation,
        base: "Test GB".into(),
        tag: Some("Eng".into()),
    };
    lib.identify_derived(&row, "Game Boy", &d).unwrap();

    let system = "Nintendo%20-%20Game%20Boy";
    let mut files = HashMap::new();
    files.insert(
        format!("/meta/genre/{system}.dat"),
        b"game (\n\tcomment \"Test GB\"\n\tgenre \"Puzzle\"\n)\n".to_vec(),
    );
    files.insert(
        format!("/thumbs/{system}/Named_Boxarts/Test%20GB.png"),
        b"\x89PNG\r\n\x1a\nfake".to_vec(),
    );
    let (base, _log) = serve(files);
    let cache = tempfile::tempdir().unwrap();
    let mut opts = EnrichOptions::new(cache.path());
    opts.metadata_base = format!("{base}/meta");
    opts.thumbnails_base = format!("{base}/thumbs");
    opts.art = vec![ArtKind::Boxart];
    opts.wikidata = None;
    let report = lib.enrich(&opts, |_| {}).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);

    let v = lib.library_view().unwrap();
    let row = v.rows.iter().find(|r| r.title == d.title()).unwrap();
    let detail = lib.game_detail(&row.id).unwrap();
    assert_eq!(detail.fields.get("genre").map(String::as_str), Some("Puzzle"));
    assert!(detail.art.contains_key("boxart"), "{:?}", detail.art);
}

/// A Redump-only system (no No-Intro DAT) still gets libretro's art under
/// libretro's own name for it, and its other sources run.
#[test]
fn redump_only_systems_get_art() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("roms");
    std::fs::create_dir(&dir).unwrap();
    let disc = vec![3u8; 4096];
    std::fs::write(dir.join("melee.iso"), &disc).unwrap();
    let h = romforge_core::hash::hash_stream(&disc[..], disc.len() as u64)
        .unwrap()
        .raw;
    let dat = tmp.path().join("gc.dat");
    std::fs::write(
        &dat,
        format!(
            r#"<datafile><header><name>Nintendo - GameCube</name></header><game name="Melee (USA)"><rom name="Melee (USA).iso" size="{}" crc="{}" md5="{}" sha1="{}"/></game></datafile>"#,
            h.size, h.crc32, h.md5, h.sha1
        ),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, Some("GameCube")).unwrap();
    lib.scan(&dir, Scope::All).unwrap();

    let mut files = HashMap::new();
    files.insert(
        "/thumbs/Nintendo%20-%20GameCube/Named_Boxarts/Melee%20(USA).png".to_owned(),
        b"\x89PNG\r\n\x1a\nfake".to_vec(),
    );
    let (base, _log) = serve(files);
    let cache = tempfile::tempdir().unwrap();
    let mut opts = EnrichOptions::new(cache.path());
    opts.metadata_base = format!("{base}/meta");
    opts.thumbnails_base = format!("{base}/thumbs");
    opts.art = vec![ArtKind::Boxart];
    opts.wikidata = None;
    let report = lib.enrich(&opts, |_| {}).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.art_saved, 1, "{report:?}");
}
