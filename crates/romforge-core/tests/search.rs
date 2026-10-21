//! Milestone 8: the Search queue on the fixtures.

use romforge_core::search::Need;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope, Status};

mod common;
use common::fixtures;

#[test]
fn queue_suggest_identify_forget() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("roms");
    std::fs::create_dir(&dir).unwrap();
    // A GB file nobody's DAT knows, named like a hack of a real game.
    std::fs::write(dir.join("Test GB (Hack).gb"), vec![42u8; 1024]).unwrap();
    std::fs::copy(fixtures().join("roms/gb/Test GB.gb"), dir.join("Test GB.gb")).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    for e in std::fs::read_dir(fixtures().join("dats")).unwrap() {
        lib.import_dat(e.unwrap().path(), None).unwrap();
    }
    lib.scan(&dir, Scope::All).unwrap();

    let q = lib.search_queue(None).unwrap();
    assert_eq!(q.len(), 1, "only the unknown file needs you: {q:?}");
    assert_eq!(q[0].need, Need::Identify);
    let row = q[0].row_id.clone();

    let dat = romforge_core::search::Scraper::Dat;
    let s = lib.search_find(&row, dat, None, None, "").unwrap();
    assert_eq!(s[0].title, "Test GB", "{s:?}");
    assert_eq!(s[0].system.as_deref(), Some("Game Boy"), "only systems using .gb");
    let typed = lib.search_find(&row, dat, Some("md5 only"), None, "").unwrap();
    assert_eq!(typed[0].title, "MD5 Only");

    lib.identify_as(&row, "Game Boy", "Test GB Deluxe (Hack)")
        .unwrap();
    let v = lib.library_view().unwrap();
    let mine = v
        .rows
        .iter()
        .find(|r| r.title == "Test GB Deluxe (Hack)")
        .unwrap();
    assert_eq!(mine.status, RowStatus::ByYou);
    assert_eq!(mine.system.as_deref(), Some("Game Boy"));
    let files = lib.files(&Default::default()).unwrap();
    assert!(files.iter().any(|f| f.status == Status::Manual));
    assert!(
        files
            .iter()
            .all(|f| f.status != Status::Matched || f.path.ends_with("Test GB.gb"))
    );
    assert!(lib.search_queue(None).unwrap().is_empty());
    // Play finds it too (it fails only for want of an emulator here).
    let nothing = romforge_core::launch::Env {
        path: vec![],
        core_dirs: vec![],
        flatpak_dirs: vec![],
        ..Default::default()
    };
    let err = lib
        .launch_plan(&nothing, "Test GB Deluxe (Hack)", None)
        .unwrap_err()
        .to_string();
    assert!(err.contains("no emulator"), "{err}");

    // Survives a rehash (it's keyed by content), and can be undone.
    lib.scan_with(
        &dir,
        Scope::All,
        &romforge_core::ScanOptions {
            rehash: true,
            ..Default::default()
        },
        |_| {},
    )
    .unwrap();
    let id = lib
        .library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.status == RowStatus::ByYou)
        .unwrap()
        .id;
    lib.forget_identity(&id).unwrap();
    assert!(
        lib.files(&Default::default())
            .unwrap()
            .iter()
            .all(|f| f.status != Status::Manual)
    );

    // Hash-verified games can't be renamed by hand.
    let verified = lib
        .library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.title == "Test GB")
        .unwrap()
        .id;
    assert!(lib.identify_as(&verified, "Game Boy", "Something else").is_err());
    assert!(
        lib.settle_wikidata("http://127.0.0.1:1/sparql", &verified, "not-a-qid")
            .is_err()
    );
}

#[test]
fn find_preview_and_choose_a_dat_title() {
    use romforge_core::enrich::EnrichOptions;
    use romforge_core::search::Scraper;

    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("roms");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("Test GB (Hack).gb"), vec![42u8; 1024]).unwrap();
    let db = tmp.path().join("romforge.db");
    let mut lib = Library::open(&db).unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(&dir, Scope::All).unwrap();
    let row = lib.search_queue(None).unwrap()[0].row_id.clone();

    let found = lib
        .search_find(&row, Scraper::Dat, None, None, "http://127.0.0.1:1")
        .unwrap();
    assert_eq!(found[0].title, "Test GB", "{found:?}");
    assert_eq!(found[0].system.as_deref(), Some("Game Boy"));

    // Nothing to fetch the picture from: the preview still comes, with the
    // DAT's ROMs set against the file.
    let cache = tmp.path().join("cache");
    let p = lib
        .search_preview(&row, &found[0], &cache, "http://127.0.0.1:1")
        .unwrap();
    assert!(p.image.is_none());
    assert!(
        p.facts
            .iter()
            .any(|(k, v)| k == "DAT ROM" && v.starts_with("Test GB.gb · 1 KiB")),
        "{p:?}"
    );
    let mine = p.facts.iter().find(|(k, _)| k == "Your file").unwrap();
    assert!(
        mine.1.contains("same size as DAT ROM, different contents"),
        "{mine:?}"
    );

    let mut opts = EnrichOptions::new(&cache);
    opts.art = Vec::new();
    opts.wikidata = None;
    opts.metadata_base = "http://127.0.0.1:1".into();
    // Its name says hack: taken as a hack of Test GB.
    let chosen = lib.search_choose(&row, &found[0], None, &opts).unwrap();
    assert_eq!(
        chosen.row_id,
        format!(
            "m:{}:Test GB [Hack]",
            romforge_core::systems::by_key("Game Boy").unwrap().id
        )
    );
    assert_eq!(chosen.enrich.games, 2, "fetches for that game and its base only");
    let d = lib.game_detail(&chosen.row_id).unwrap();
    assert_eq!(d.row.status, RowStatus::Derivative);
    assert_eq!(d.derived.unwrap().base, "Test GB");
    // Wikidata has no item for a hack; it shows its base game's, so a
    // lookup that found nothing under its own name asks nothing of you.
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute(
            "INSERT INTO wikidata_match (system_id, title, status, candidates, checked_at)
             SELECT system_id, 'Test GB [Hack]', 'none', '[]', '' FROM systems WHERE key = 'Game Boy'",
            [],
        )
        .unwrap();
    assert!(
        lib.search_queue(None)
            .unwrap()
            .iter()
            .all(|q| q.row_id != chosen.row_id),
        "a derivative needs no Wikidata match"
    );

    // A Wikidata pick on an unidentified row needs a platform.
    let other = tmp.path().join("other");
    std::fs::create_dir(&other).unwrap();
    std::fs::write(other.join("mystery.bin"), vec![7u8; 64]).unwrap();
    lib.scan(&other, Scope::All).unwrap();
    let row = lib
        .search_queue(None)
        .unwrap()
        .into_iter()
        .find(|q| q.title.contains("mystery"))
        .unwrap()
        .row_id;
    let wd = romforge_core::search::Found {
        scraper: Scraper::Wikidata,
        id: "Q1".into(),
        title: "Mystery".into(),
        system: None,
        year: None,
        score: 1.0,
        description: None,
        band: None,
        evidence: Vec::new(),
        derived: None,
    };
    let err = lib.search_choose(&row, &wd, None, &opts).unwrap_err().to_string();
    assert!(err.contains("platform"), "{err}");
}

/// A pick in Search takes effect on a file that has only a CRC32 hit (a
/// candidate); on files already matched by hash it says so.
#[test]
fn picks_outrank_crc_only_hits() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("roms");
    std::fs::create_dir(&dir).unwrap();
    std::fs::copy(fixtures().join("roms/gb/CRC Only.gb"), dir.join("CRC Only.gb")).unwrap();
    std::fs::copy(fixtures().join("roms/gb/Test GB.gb"), dir.join("Test GB.gb")).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(&dir, Scope::All).unwrap();
    let rows = lib.library_view().unwrap().rows;
    let crc = rows
        .iter()
        .find(|r| r.status == RowStatus::Candidate)
        .expect("a candidate")
        .id
        .clone();
    lib.identify_as(&crc, "Game Boy", "CRC Only (My Copy)").unwrap();
    let rows = lib.library_view().unwrap().rows;
    let mine = rows
        .iter()
        .find(|r| r.title == "CRC Only (My Copy)")
        .expect("named");
    assert_eq!(mine.status, RowStatus::ByYou);
    let verified = rows.iter().find(|r| r.title == "Test GB").unwrap().id.clone();
    let err = lib
        .identify_as(&verified, "Game Boy", "Other")
        .unwrap_err()
        .to_string();
    assert!(err.contains("already identified by its hashes"), "{err}");
}

/// Naming several rows by their suggestions at once: a translation is
/// named as one (never verified); a file with no good suggestion is left.
#[test]
fn naming_selected_rows_as_suggested() {
    use romforge_core::view::RowStatus;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("roms");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(
        tmp.path().join("snes.dat"),
        r#"<datafile><header><name>Nintendo - Super Nintendo Entertainment System</name></header>
  <game name="Live A Live (Japan)"><rom name="l.sfc" size="2048" crc="00000004"/></game>
</datafile>"#,
    )
    .unwrap();
    std::fs::write(dir.join("Live A Live English v2.00.sfc"), vec![5u8; 4096]).unwrap();
    std::fs::write(dir.join("Nothing Like It.sfc"), vec![6u8; 4096]).unwrap();
    let mut lib = romforge_core::Library::open_in_memory().unwrap();
    lib.import_dat(tmp.path().join("snes.dat"), None).unwrap();
    lib.scan(&dir, romforge_core::Scope::All).unwrap();
    let ids: Vec<String> = lib
        .library_view()
        .unwrap()
        .rows
        .iter()
        .map(|r| r.id.clone())
        .collect();
    let done = lib.search_accept(&ids).unwrap();
    let live = done.iter().find(|a| a.file.starts_with("Live")).unwrap();
    assert!(
        live.named
            .as_deref()
            .is_some_and(|n| n.starts_with("Live A Live (Japan)")),
        "{done:?}"
    );
    let other = done.iter().find(|a| a.file.starts_with("Nothing")).unwrap();
    assert!(other.named.is_none() && other.why_not.is_some(), "{done:?}");
    let rows = lib.library_view().unwrap().rows;
    // Listed under the original you don't have, as its translation.
    let row = rows
        .iter()
        .find(|r| r.status == RowStatus::Derivative)
        .unwrap_or_else(|| panic!("{rows:?}"));
    assert!(row.title.starts_with("Live A Live (Japan)"), "{row:?}");
    // Named once: asking again leaves it as it is.
    let again = lib.search_accept(std::slice::from_ref(&row.id)).unwrap();
    assert_eq!(again[0].why_not.as_deref(), Some("already identified"));
}
