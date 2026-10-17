//! Wikidata enrichment against a local stand-in for the SPARQL endpoint.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use romforge_core::enrich::EnrichOptions;
use romforge_core::{Library, Scope};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn uri(q: &str) -> String {
    format!(r#"{{"type":"uri","value":"http://www.wikidata.org/entity/{q}"}}"#)
}

fn lit(v: &str) -> String {
    format!(r#"{{"type":"literal","value":"{v}"}}"#)
}

/// Answers SPARQL queries by what they ask for; records each query.
fn serve() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/sparql", listener.local_addr().unwrap());
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = log.clone();
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
            let target = line.split_whitespace().nth(1).unwrap_or("").to_owned();
            let query = target
                .split_once("query=")
                .map(|(_, q)| percent_decode(q))
                .unwrap_or_default();
            seen.lock().unwrap().push(query.clone());
            let rows: Vec<String> = if query.contains("rdfs:label ?label") {
                // Game Boy games: one exact match, one ambiguous pair.
                vec![
                    format!(
                        r#"{{"g":{},"label":{},"date":{}}}"#,
                        uri("Q100"),
                        lit("Test GB"),
                        lit("1990-01-01T00:00:00Z")
                    ),
                    format!(
                        r#"{{"g":{},"label":{},"alias":{}}}"#,
                        uri("Q201"),
                        lit("Shared Hash"),
                        lit("Shared Hash (Rev A)")
                    ),
                    format!(
                        r#"{{"g":{},"label":{},"alias":{}}}"#,
                        uri("Q202"),
                        lit("Shared Hash Deluxe"),
                        lit("Shared Hash (Rev A)")
                    ),
                ]
            } else if query.contains("schema:about") {
                vec![
                    format!(
                        r#"{{"g":{},"p":{},"v":{},"vLabel":{}}}"#,
                        uri("Q100"),
                        r#"{"type":"uri","value":"http://www.wikidata.org/prop/direct/P178"}"#,
                        uri("Q9"),
                        lit("Test Studio")
                    ),
                    format!(
                        r#"{{"g":{},"p":{},"v":{},"vLabel":{}}}"#,
                        uri("Q100"),
                        r#"{"type":"uri","value":"http://www.wikidata.org/prop/direct/P11688"}"#,
                        lit("4242"),
                        lit("4242")
                    ),
                    format!(
                        r#"{{"g":{},"p":{},"v":{},"vLabel":{}}}"#,
                        uri("Q100"),
                        lit("series"),
                        uri("Q500"),
                        lit("Test Saga")
                    ),
                    format!(
                        r#"{{"g":{},"p":{},"v":{}}}"#,
                        uri("Q100"),
                        lit("series_ordinal"),
                        lit("2")
                    ),
                    format!(
                        r#"{{"g":{},"p":{},"v":{}}}"#,
                        uri("Q100"),
                        lit("wikipedia"),
                        lit("Test GB (video game)")
                    ),
                ]
            } else if query.contains("GROUP BY ?s ?m") {
                vec![
                    format!(
                        r#"{{"s":{},"m":{},"mLabel":{},"ord":{},"first":{}}}"#,
                        uri("Q500"),
                        uri("Q99"),
                        lit("Test Zero"),
                        lit("1"),
                        lit("1988-05-01T00:00:00Z")
                    ),
                    format!(
                        r#"{{"s":{},"m":{},"mLabel":{},"ord":{},"first":{}}}"#,
                        uri("Q500"),
                        uri("Q100"),
                        lit("Test GB"),
                        lit("2"),
                        lit("1990-01-01T00:00:00Z")
                    ),
                    format!(
                        r#"{{"s":{},"m":{},"mLabel":{},"ord":{},"first":{}}}"#,
                        uri("Q500"),
                        uri("Q101"),
                        lit("Test GB 2"),
                        lit("3"),
                        lit("1993-02-01T00:00:00Z")
                    ),
                ]
            } else {
                Vec::new()
            };
            let body = format!(r#"{{"results":{{"bindings":[{}]}}}}"#, rows.join(","));
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/sparql-results+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.flush();
            let mut sink = [0u8; 1];
            let _ = stream.read(&mut sink);
        }
    });
    (base, log)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap_or(b'?'));
            i += 3;
        } else {
            out.push(if b[i] == b'+' { b' ' } else { b[i] });
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn matches_fills_and_lists_the_series() {
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(fixtures().join("roms/gb"), Scope::All).unwrap();
    let (endpoint, log) = serve();
    let cache = tempfile::tempdir().unwrap();
    let mut opts = EnrichOptions::new(cache.path());
    opts.art = Vec::new();
    // No libretro: point its base at nothing so only Wikidata answers.
    opts.metadata_base = format!("{endpoint}/none");
    opts.wikidata = Some(endpoint);

    let report = lib.enrich(&opts, |_| {}).unwrap();
    let w = report.wikidata.expect("Wikidata ran");
    assert!(w.errors.is_empty(), "{:?}", w.errors);
    assert_eq!(w.matched, 1, "{w:?}");
    assert!(
        w.ambiguous >= 1,
        "Shared Hash (Rev A) has two equal candidates: {w:?}"
    );

    let v = lib.library_view().unwrap();
    let id = &v.rows.iter().find(|r| r.title == "Test GB").unwrap().id;
    let d = lib.game_detail(id).unwrap();
    assert_eq!(d.fields.get("developer").map(String::as_str), Some("Test Studio"));
    assert_eq!(d.fields.get("wikidata").map(String::as_str), Some("Q100"));
    let titles: Vec<&str> = d.series.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, ["Test Zero", "Test GB", "Test GB 2"], "in series order");
    assert!(d.series[1].current && d.series[1].in_library);
    assert!(!d.series[0].in_library);
    let moby = d.links.iter().find(|l| l.label == "MobyGames").unwrap();
    assert!(moby.direct && moby.url == "https://www.mobygames.com/game/4242/");
    let wiki = d.links.iter().find(|l| l.label == "Wikipedia").unwrap();
    assert_eq!(wiki.url, "https://en.wikipedia.org/wiki/Test_GB_(video_game)");

    // A second run asks Wikidata only for details and series, not for
    // candidates again (the matches are settled).
    let before = log.lock().unwrap().len();
    lib.enrich(&opts, |_| {}).unwrap();
    let again: Vec<String> = log.lock().unwrap()[before..].to_vec();
    assert!(
        again.iter().all(|q| !q.contains("rdfs:label ?label")),
        "{again:?}"
    );
}
