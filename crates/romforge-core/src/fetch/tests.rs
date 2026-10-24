use super::*;
use crate::systems;

#[test]
fn sources_come_from_the_systems_table() {
    let psx = systems::by_key("PlayStation").unwrap();
    let urls: Vec<_> = sources(psx, false).iter().map(|s| s.url().unwrap()).collect();
    assert_eq!(
        urls,
        [
            "http://redump.org/datfile/psx/serial,version",
            "http://redump.org/datfile/psx-bios/serial,version",
            "https://raw.githubusercontent.com/unexpectedpanda/retool-clonelists-metadata/main/clonelists/Sony%20-%20PlayStation%20(Redump).json"
        ]
    );
    assert_eq!(sources(psx, true).len(), 4, "plus the PlayStation hacks DAT");
    let gba = sources(systems::by_key("Game Boy Advance").unwrap(), false);
    assert_eq!(
        gba[0].url().unwrap(),
        "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/no-intro/Nintendo%20-%20Game%20Boy%20Advance.dat"
    );
    assert_eq!(
        gba[2],
        Source::Serials("Nintendo - Game Boy Advance"),
        "after both No-Intro DATs"
    );
    let c64 = sources(systems::by_key("Commodore 64").unwrap(), false);
    let tosec = c64.iter().find(|s| matches!(s, Source::Tosec(_))).unwrap();
    assert_eq!(tosec, &Source::Tosec("Commodore C64"));
    assert_eq!(tosec.url(), None);
    assert!(sources(systems::by_key("Arcade").unwrap(), true).is_empty());
}

#[test]
fn finds_latest_tosec_release_and_pack_link() {
    let index = r#"<a href="/downloads/category/22-datfiles">x</a>
        <a href="/downloads/category/59-2025-03-13">x</a>
        <a href="/downloads/category/58-2024-03-31">x</a>
        <a href="/downloads/category/8-tosec-dat-explorer">x</a>"#;
    assert_eq!(
        latest_tosec_release(index).as_deref(),
        Some("/downloads/category/59-2025-03-13")
    );
    let page = r#"<a href="/downloads/category/22-datfiles">x</a>
        <a href="/downloads/category/59-2025-03-13?download=117:tosec-dat-pack-complete-4743-tosec-v2025-03-13&amp;x=1">DAT</a>"#;
    assert_eq!(
        tosec_pack_link(page).as_deref(),
        Some(
            "/downloads/category/59-2025-03-13?download=117:tosec-dat-pack-complete-4743-tosec-v2025-03-13&x=1"
        )
    );
    assert_eq!(latest_tosec_release("<html></html>"), None);
}

#[test]
fn source_families() {
    assert_eq!(
        source_family("https://www.tosecdev.org/a?download=1#TOSEC/x.dat"),
        "tosec"
    );
    assert_eq!(
        source_family("http://redump.org/datfile/psx/"),
        "http://redump.org/datfile/psx/"
    );
    assert_eq!(
        source_family("http://redump.org/datfile/psx/serial,version"),
        "http://redump.org/datfile/psx/",
        "the same family with serials asked for"
    );
    assert_eq!(source_family("/home/me/x.dat"), "file");
}

#[test]
fn parses_profile() {
    let xml = r#"<clrmamepro>
  <datfile><name>Nintendo - Game Boy</name><description>x</description><version>20260101-000000</version><url>_</url></datfile>
  <datfile><name>Sega - Mega Drive - Genesis</name><version>20250505-121212</version></datfile>
</clrmamepro>"#;
    let p = parse_profile(xml.as_bytes()).unwrap();
    assert_eq!(p.len(), 2);
    assert_eq!(p[0].name, "Nintendo - Game Boy");
    assert_eq!(p[0].version, "20260101-000000");
    assert_eq!(p[1].name, "Sega - Mega Drive - Genesis");
}

#[test]
fn a_long_retry_after_is_reported_not_waited_out() {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/sparql?query=x", listener.local_addr().unwrap());
    let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = hits.clone();
    std::thread::spawn(move || {
        for s in listener.incoming() {
            let Ok(mut s) = s else { continue };
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            while r.read_line(&mut line).unwrap_or(0) > 2 {
                line.clear();
            }
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let _ = write!(
                s,
                "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1000\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
        }
    });
    let started = Instant::now();
    let e = get(&url).unwrap_err();
    assert!(is_rate_limited(&e), "{e}");
    assert!(e.to_string().contains("17 minutes"), "{e}");
    assert!(
        !e.to_string().contains("query=x"),
        "no query string in errors: {e}"
    );
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1, "asked once");
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn plain_dat_is_passed_through() {
    let d = dats_in(b"<datafile/>").unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!(
        (d[0].member.as_deref(), &d[0].bytes[..]),
        (None, &b"<datafile/>"[..])
    );
}

/// Answers every request with `head` (status line and headers) and `body`;
/// counts the requests.
fn answer(head: &'static str, body: Vec<u8>) -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = hits.clone();
    std::thread::spawn(move || {
        for s in listener.incoming() {
            let Ok(mut s) = s else { continue };
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            while r.read_line(&mut line).unwrap_or(0) > 2 {
                line.clear();
            }
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let _ = write!(
                s,
                "{head}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = s.write_all(&body);
        }
    });
    (base, hits)
}

#[test]
fn a_used_up_quota_halts_at_once() {
    for head in ["HTTP/1.1 430 Quota", "HTTP/1.1 431 Too Many Unknown"] {
        let (base, hits) = answer(head, Vec::new());
        let e = get(&format!("{base}/api2/jeuInfos.php?ssid=me&sspassword=secret")).unwrap_err();
        assert!(is_rate_limited(&e), "{e}");
        assert!(!e.to_string().contains("secret"), "{e}");
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1, "asked once");
    }
}

#[test]
fn bodies_and_downloads_have_a_cap() {
    let (base, _) = answer("HTTP/1.1 200 OK", vec![7u8; 100]);
    let url = format!("{base}/big.png");
    let small = Request {
        max_body: Some(10),
        ..Request::get(&url)
    };
    assert!(request(&small).is_err(), "over the body limit");
    assert_eq!(request(&Request::image(&url)).unwrap().unwrap().len(), 100);

    let dir = tempfile::tempdir().unwrap();
    let to = dir.path().join("x.zip");
    let e = download_to(&url, &to, 10, |_, _| {}).unwrap_err();
    assert!(e.to_string().contains("more than the 10"), "{e}");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "nothing left");
    assert_eq!(download_to(&url, &to, 1000, |_, _| {}).unwrap(), 100);
}

#[test]
fn a_waiting_host_never_holds_up_another() {
    // Two requests to one host a second apart (no request is made).
    let first = std::thread::spawn(|| {
        wait_for_host("https://slow.example.invalid/a");
        wait_for_host("https://slow.example.invalid/b");
    });
    std::thread::sleep(Duration::from_millis(200));
    let started = Instant::now();
    wait_for_host("https://other.example.invalid/");
    assert!(started.elapsed() < Duration::from_millis(500));
    first.join().unwrap();
}

#[test]
fn requests_never_print_their_secrets() {
    let r = Request {
        headers: vec![("Authorization", "Bearer s3cret".into())],
        body: Some("client_secret=s3cret".into()),
        ..Request::get("https://x.example/token?client_secret=s3cret")
    };
    let shown = format!("{r:?}");
    assert!(!shown.contains("s3cret"), "{shown}");
    assert!(shown.contains("Authorization"), "{shown}");
    assert_eq!(host_of("https://a.org:8080/x?y"), "a.org:8080");
    assert_eq!(host_of("http://a.org?q=1"), "a.org");
}

#[test]
fn userinfo_is_not_the_host() {
    let url = "https://www.screenscraper.fr:x@attacker.tld/m.pdf";
    assert_eq!(host_of(url), "attacker.tld");
    assert!(has_userinfo(url));
    assert!(!has_userinfo("https://a.org/x?mail=a@b.org"));
    assert_eq!(host_of("https://a.org/x?mail=a@b.org"), "a.org");
}

#[test]
fn the_arcade_database_gets_one_request_at_a_time() {
    assert!(request::one_at_a_time("https://example.org/a").is_none());
    let held = request::one_at_a_time("https://adb.arcadeitalia.net/service_scraper.php?x").unwrap();
    assert!(request::ONE.try_lock().is_err(), "a second request waits");
    drop(held);
    assert!(request::ONE.try_lock().is_ok());
}

#[test]
fn a_zip_of_dats_has_a_total_cap() {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for name in ["a.dat", "b.dat"] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&[b'x'; 100]).unwrap();
    }
    let bytes = zip.finish().unwrap().into_inner();
    assert_eq!(dats_in_capped(&bytes, 300).unwrap().len(), 2);
    let e = dats_in_capped(&bytes, 150).unwrap_err();
    assert!(e.to_string().contains("together"), "{e}");
}

#[test]
fn a_busy_host_holds_every_worker() {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/x", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for (i, s) in listener.incoming().enumerate() {
            let Ok(mut s) = s else { continue };
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            while r.read_line(&mut line).unwrap_or(0) > 2 {
                line.clear();
            }
            let head = if i == 0 {
                "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 2"
            } else {
                "HTTP/1.1 200 OK"
            };
            let _ = write!(s, "{head}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    });
    let first = {
        let url = url.clone();
        std::thread::spawn(move || get(&url))
    };
    std::thread::sleep(Duration::from_millis(500));
    // Another worker asking the same host waits out the Retry-After too.
    let started = Instant::now();
    wait_for_host(&url);
    assert!(
        started.elapsed() >= Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    first.join().unwrap().unwrap();
}

#[test]
fn wii_u_gets_the_digital_dat_and_the_serial_list() {
    let wiiu = sources(systems::by_key("Wii U").unwrap(), false);
    assert_eq!(wiiu[0], Source::NoIntro("Nintendo - Wii U (Digital)"));
    let list = wiiu.iter().find(|s| matches!(s, Source::SerialDat(_))).unwrap();
    assert_eq!(
        list.url().unwrap(),
        "https://raw.githubusercontent.com/libretro/libretro-database/master/dat/Nintendo%20-%20Wii%20U.dat"
    );
    assert!(list.is_list() && !wiiu[0].is_list());
}
