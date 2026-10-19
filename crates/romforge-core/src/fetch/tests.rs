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
