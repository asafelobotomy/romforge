use super::*;

const LISTING: &str = r#"<table><tr><th>Name</th></tr>
<tr><td><a href="//archive.org/download/retrokit-manuals/snes/snes-compressed.zip/Super%20Mario%20Kart%20%28en%29.pdf">Super Mario Kart (en).pdf</a><td><td>2022-02-01 22:05<td id="size">4271183</tr>
<tr><td><a href="//archive.org/download/retrokit-manuals/snes/snes-compressed.zip/Super%20Mario%20Kart%20%28ja%29.pdf">Super Mario Kart (ja).pdf</a><td><td>2022-02-01 22:05<td id="size">5000</tr>
<tr><td><a href="//archive.org/download/retrokit-manuals/snes/snes-compressed.zip/Super%20Mario%20Kart%20%28en-gb%2Cde%2Cfr%29.pdf">Super Mario Kart (en-gb,de,fr).pdf</a><td><td>2022-02-01 22:05<td id="size">6000</tr>
<tr><td><a href="//archive.org/download/retrokit-manuals/snes/snes-compressed.zip/7th%20Saga%2C%20The%20%28en%29.pdf">7th Saga, The (en).pdf</a><td><td>2022-02-01 22:05<td id="size">5593167</tr>
<tr><td><a href="//archive.org/download/retrokit-manuals/snes/snes-compressed.zip/Tom%20%26amp%3B%20Jerry%20%28en%29.pdf">Tom &amp; Jerry (en).pdf</a><td><td>2022-02-01 22:05<td id="size">10</tr>
</table>"#;

#[test]
fn retrokit_listing_is_read() {
    let l = parse_listing(LISTING);
    assert_eq!(l.len(), 5);
    assert_eq!(l[0], ("Super Mario Kart (en).pdf".into(), Some(4_271_183)));
    assert_eq!(l[4].0, "Tom & Jerry (en).pdf");
}

#[test]
fn a_game_matches_by_title_with_its_language_first() {
    let l = parse_listing(LISTING);
    let names = |t: &str| {
        retrokit_matches("snes", &l, t, None)
            .into_iter()
            .map(|o| o.langs)
            .collect::<Vec<_>>()
    };
    assert_eq!(names("Super Mario Kart (USA)"), ["en", "en-gb,de,fr", "ja"]);
    assert_eq!(names("Super Mario Kart (Japan)"), ["ja", "en", "en-gb,de,fr"]);
    assert_eq!(names("Super Mario Kart (Europe)"), ["en-gb,de,fr", "en", "ja"]);
    assert_eq!(names("7th Saga, The (USA)"), ["en"]);
    assert!(names("Super Mario World (USA)").is_empty(), "no near misses");
    let clone = retrokit_matches(
        "snes",
        &l,
        "Mario Kart Super (Brazil)",
        Some("Super Mario Kart (USA)"),
    );
    assert_eq!(clone.len(), 3, "a clone finds its parent's manual");
    let o = &retrokit_matches("snes", &l, "7th Saga, The (USA)", None)[0];
    assert_eq!(
        o.url,
        "https://archive.org/download/retrokit-manuals/snes/snes-compressed.zip/7th%20Saga%2C%20The%20(en).pdf"
    );
    assert_eq!(o.size, Some(5_593_167));
}

#[test]
fn kirkland_answers_are_read() {
    let search = br#"{"response":{"numFound":1,"docs":[{"identifier":"snes_Super_Mario_Kart_USA","title":"Super Mario Kart (USA)"}]}}"#;
    assert_eq!(search_ids(search), ["snes_Super_Mario_Kart_USA"]);
    let files = br#"{"result":[{"name":"Super_Mario_Kart_USA_text.pdf","source":"derivative","size":"946500"},{"name":"Super_Mario_Kart_USA.pdf","source":"original","size":"4271183"}]}"#;
    assert_eq!(
        item_pdf(files),
        Some(("Super_Mario_Kart_USA.pdf".into(), Some(4_271_183)))
    );
    assert!(
        kirkland("No Region").unwrap().is_empty(),
        "only exact No-Intro names are searched"
    );
}

#[test]
fn every_manual_folder_is_used_once() {
    let mut seen = std::collections::HashSet::new();
    for s in crate::systems::SYSTEMS {
        for f in s.manuals {
            assert!(seen.insert(*f), "{f} is on two systems");
        }
    }
    assert!(seen.len() >= 40);
}

#[test]
fn only_archive_org_downloads_are_taken() {
    let t = tempfile::tempdir().unwrap();
    let conn = crate::db::open_in_memory().unwrap();
    let offer = ManualOffer {
        source: "x".into(),
        name: "x.pdf".into(),
        langs: String::new(),
        url: "https://example.com/x.pdf".into(),
        size: None,
    };
    let keys = crate::settings::Keys::default();
    assert!(download_offer(&conn, t.path(), t.path(), "SNES", "G", &offer, &keys).is_err());
    assert!(super::known_source(
        "https://adb.arcadeitalia.net/download_file.php?x=1"
    ));
    assert!(super::known_source(
        "https://neoclone.screenscraper.fr/api2/mediaManuelJeu.php?x"
    ));
    assert!(
        !super::known_source("http://adb.arcadeitalia.net/x.pdf"),
        "https only"
    );
    assert!(!super::known_source("https://evil.example/screenscraper.fr.pdf"));
    assert!(
        !super::known_source("https://www.screenscraper.fr:x@attacker.tld/m.pdf"),
        "userinfo hides the real host"
    );
}
