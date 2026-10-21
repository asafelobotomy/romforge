//! The app's toolbar: Scan folder (the first one becomes the Source), its
//! dry run (the library unchanged), Refresh (only Settings' folders), and
//! Enrich on the selected games only. Always on copies in temp folders.

use std::path::{Path, PathBuf};

use romforge_core::Library;
use romforge_core::scan::ScanOptions;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn folder(dir: &Path, name: &str, files: &[&str]) -> PathBuf {
    let d = dir.join(name);
    std::fs::create_dir_all(&d).unwrap();
    for f in files {
        let from = fixtures().join("roms").join(f);
        std::fs::copy(&from, d.join(from.file_name().unwrap())).unwrap();
    }
    d.canonicalize().unwrap()
}

fn titles(lib: &Library) -> Vec<String> {
    lib.library_view()
        .unwrap()
        .rows
        .into_iter()
        .map(|r| r.title)
        .collect()
}

#[test]
fn the_first_folder_scanned_becomes_the_source() {
    let tmp = tempfile::tempdir().unwrap();
    let a = folder(tmp.path(), "a", &["gb/Test GB.gb"]);
    let b = folder(tmp.path(), "b", &["nes/Test NES (iNES).nes"]);
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.import_dat(fixtures().join("dats/nes.dat"), None).unwrap();
    assert!(!lib.can_refresh().unwrap(), "nothing to refresh yet");
    let first = lib.scan_folder(&a).unwrap();
    assert_eq!(first.became_source, Some(Ok(())));
    assert_eq!(lib.settings().unwrap().source_dir.as_deref(), Some(a.as_path()));
    assert!(lib.can_refresh().unwrap());
    let second = lib.scan_folder(&b).unwrap();
    assert_eq!(second.became_source, None, "a Source is set: it stays");
    assert_eq!(lib.settings().unwrap().source_dir.as_deref(), Some(a.as_path()));
    assert!(
        titles(&lib).contains(&"Test NES (iNES)".to_owned()),
        "b was scanned all the same"
    );
}

#[test]
fn a_dry_run_scan_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let a = folder(tmp.path(), "a", &["gb/Test GB.gb", "unknown.bin"]);
    let db = tmp.path().join("library.db");
    let mut lib = Library::open(&db).unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    let before = titles(&lib);
    let p = lib.scan_preview(&a).unwrap();
    assert_eq!(p.report.hashed, 2);
    let found: Vec<(&str, bool)> = p.games.iter().map(|g| (g.title.as_str(), g.new)).collect();
    assert!(found.contains(&("Test GB", true)), "{found:?}");
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(titles(&lib), before, "the library as it was");
    assert!(
        lib.settings().unwrap().source_dir.is_none(),
        "no Source set by a dry run"
    );
    let leftovers: Vec<_> = std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(&format!("romforge-preview-{}", std::process::id()))
        })
        .collect();
    assert!(leftovers.is_empty(), "the copy is gone");
}

#[test]
fn refresh_rescans_only_the_settings_folders() {
    let tmp = tempfile::tempdir().unwrap();
    let source = folder(tmp.path(), "source", &["gb/Test GB.gb"]);
    let other = folder(tmp.path(), "other", &["nes/Test NES (iNES).nes"]);
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.import_dat(fixtures().join("dats/nes.dat"), None).unwrap();
    lib.scan_folder(&source).unwrap();
    lib.scan_folder(&other).unwrap();
    // Gone from both folders.
    std::fs::remove_file(source.join("Test GB.gb")).unwrap();
    std::fs::remove_file(other.join("Test NES (iNES).nes")).unwrap();
    let done = lib.refresh_settings(&ScanOptions::default(), |_, _| {}).unwrap();
    assert_eq!(done.len(), 1, "only the Source");
    let t = titles(&lib);
    assert!(!t.contains(&"Test GB".to_owned()), "the Source was rescanned");
    assert!(
        t.contains(&"Test NES (iNES)".to_owned()),
        "the other folder wasn't"
    );
    // A full refresh (after a cleanup, or the CLI's) still takes every folder.
    lib.refresh(&ScanOptions::default(), |_, _| {}).unwrap();
    assert!(!titles(&lib).contains(&"Test NES (iNES)".to_owned()));
}

/// Platform icons come once from retroarch-assets, by the theme in
/// Settings: one it has is kept, one it hasn't is remembered and not asked
/// for again; "none" asks for nothing.
#[test]
fn platform_icons_are_fetched_once() {
    use std::io::{BufRead, BufReader, Write};
    use std::sync::{Arc, Mutex};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let base = format!("{host}/xmb");
    let sparql = format!("{host}/sparql");
    let served = host.clone();
    let asked: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = asked.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                    break;
                }
            }
            let path = line.split_whitespace().nth(1).unwrap_or("").to_owned();
            log.lock().unwrap().push(path.clone());
            // Wikidata knows the Game Boy's logo (its item Q186437).
            let logo = format!(
                r#"{{"results":{{"bindings":[{{"e":{{"type":"uri","value":"http://www.wikidata.org/entity/Q186437"}},"logo":{{"type":"uri","value":"{served}/commons/GB%20logo.svg"}}}}]}}}}"#
            );
            let body: &[u8] = if path == "/xmb/flatui/png/Nintendo%20-%20Game%20Boy.png"
                || path.starts_with("/commons/GB%20logo.svg?width=256")
            {
                b"\x89PNG\r\n\x1a\nicon"
            } else if path.starts_with("/sparql") {
                logo.as_bytes()
            } else {
                b""
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
        }
    });
    let tmp = tempfile::tempdir().unwrap();
    let a = folder(tmp.path(), "a", &["gb/Test GB.gb", "nes/Test NES (iNES).nes"]);
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.import_dat(fixtures().join("dats/nes.dat"), None).unwrap();
    lib.scan_folder(&a).unwrap();
    let cache = tmp.path().join("cache");
    let got = lib.platform_icons(&cache, &base, &sparql).unwrap();
    assert_eq!(got.theme, "flatui", "controllers by default");
    let icons = got.icons;
    assert_eq!(icons.keys().collect::<Vec<_>>(), ["Game Boy"]);
    assert!(icons["Game Boy"].exists());
    let first = asked.lock().unwrap().len();
    assert_eq!(first, 2, "{:?}", asked.lock().unwrap());
    lib.platform_icons(&cache, &base, &sparql).unwrap();
    assert_eq!(asked.lock().unwrap().len(), first, "neither asked again");
    // Logos: from Wikidata, as small PNGs Commons renders.
    let mut s = lib.settings().unwrap();
    s.platform_icons = "logo".into();
    lib.save_settings(&s).unwrap();
    let logos = lib.platform_icons(&cache, &base, &sparql).unwrap();
    assert_eq!(logos.theme, "logo");
    assert_eq!(
        logos.icons.keys().collect::<Vec<_>>(),
        ["Game Boy"],
        "the NES has none here"
    );
    assert!(logos.icons["Game Boy"].to_string_lossy().contains("/logo/"));
    s.platform_icons = "none".into();
    lib.save_settings(&s).unwrap();
    assert!(
        lib.platform_icons(&cache, &base, &sparql)
            .unwrap()
            .icons
            .is_empty()
    );
    s.platform_icons = "pixel".into();
    assert!(
        lib.save_settings(&s).is_err(),
        "controllers, systems or logos only"
    );
}

#[test]
fn enrich_takes_only_the_selected_games() {
    use romforge_core::enrich::EnrichOptions;
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.import_dat(fixtures().join("dats/nes.dat"), None).unwrap();
    lib.scan(fixtures().join("roms/gb"), romforge_core::Scope::All)
        .unwrap();
    lib.scan(fixtures().join("roms/nes"), romforge_core::Scope::All)
        .unwrap();
    let rows = lib.library_view().unwrap().rows;
    let gb = rows.iter().find(|r| r.title == "Test GB").unwrap().id.clone();

    // Nothing answers: only which games were taken matters here.
    let cache = tempfile::tempdir().unwrap();
    let mut opts = EnrichOptions::new(cache.path());
    let nowhere = "http://127.0.0.1:9".to_owned();
    opts.metadata_base = nowhere.clone();
    opts.dats_base = nowhere.clone();
    opts.thumbnails_base = nowhere;
    opts.wikidata = None;
    let all = lib.enrich(&opts, |_| {}).unwrap();
    assert!(all.games > 1, "{all:?}");
    let one = lib.enrich_rows(&opts, &[gb], |_| {}).unwrap();
    assert_eq!((one.systems, one.games), (1, 1), "{one:?}");
    // A row that isn't in the list takes nothing.
    let none = lib.enrich_rows(&opts, &["no such row".into()], |_| {}).unwrap();
    assert_eq!(none.games, 0);
}
