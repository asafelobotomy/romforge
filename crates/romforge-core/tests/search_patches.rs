//! Milestone 8: the Search queue on the fixtures.
//!
//! Translations, hacks and patches.

use romforge_core::search::Need;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope};

mod common;
use common::search::*;

/// A translation is suggested as "translation of X" and, once taken, has a
/// status of its own; a patch beside a file proves its base.
#[test]
fn translations_and_hacks() {
    use romforge_core::enrich::EnrichOptions;
    use romforge_core::search::Scraper;
    use romforge_core::suggest::Band;
    use romforge_core::view::Relation;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("roms");
    std::fs::create_dir(&dir).unwrap();
    let original = snes_rom(b"DRAGONQUEST3         ", b"AQ3J", 0, 0x10000, 0x11);
    let h = romforge_core::hash::hash_stream(&original[..], original.len() as u64)
        .unwrap()
        .raw;
    let dat = tmp.path().join("snes.dat");
    std::fs::write(
        &dat,
        format!(
            r#"<datafile><header><name>Nintendo - Super Nintendo Entertainment System</name></header>
  <game name="Dragon Quest III - Soshite Densetsu e... (Japan)"><rom name="dq3.sfc" size="{}" crc="{}" md5="{}" sha1="{}"/></game>
</datafile>"#,
            h.size, h.crc32, h.md5, h.sha1
        ),
    )
    .unwrap();
    let translated = snes_rom(b"DRAGONQUEST3         ", b"AQ3J", 0, 0x18000, 0x22);
    std::fs::write(
        dir.join("Dragon Quest III (English v2.0)[Addendum].sfc"),
        &translated,
    )
    .unwrap();
    // A file whose name says nothing, with the patch that made it.
    let patched = vec![0x5Au8; 0x10000];
    std::fs::write(dir.join("dq3 fixed.sfc"), &patched).unwrap();
    std::fs::write(dir.join("dq3 fixed.bps"), bps(&original, &patched)).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.scan(&dir, Scope::All).unwrap();

    let q = lib.search_queue(None).unwrap();
    let item = |needle: &str| q.iter().find(|i| i.title.contains(needle)).unwrap().clone();
    let tr = item("English").suggestion.unwrap();
    let d = tr.derived.clone().expect("its name says translation");
    assert_eq!(d.relation, Relation::Translation);
    assert_eq!(d.base, "Dragon Quest III - Soshite Densetsu e... (Japan)");
    assert_eq!(d.tag.as_deref(), Some("English v2.0 Addendum"));

    let fixed = item("dq3 fixed").suggestion.expect("the patch tells");
    assert_eq!(fixed.band, Band::LikelyModified);
    assert_eq!(fixed.title, "Dragon Quest III - Soshite Densetsu e... (Japan)");
    assert!(
        fixed
            .evidence
            .iter()
            .any(|e| e.kind == "patch" && e.text.contains("turns the DAT ROM into this file"))
    );
    assert_eq!(fixed.derived.map(|d| d.relation), Some(Relation::Hack));

    // Taking the translation's suggestion in Search.
    let row = item("English").row_id;
    let found = lib
        .search_find(&row, Scraper::Dat, None, None, "http://127.0.0.1:1")
        .unwrap()
        .into_iter()
        .find(|f| f.derived.is_some())
        .unwrap();
    let mut opts = EnrichOptions::new(tmp.path().join("cache"));
    opts.art = Vec::new();
    opts.wikidata = None;
    opts.metadata_base = "http://127.0.0.1:1".into();
    opts.thumbnails_base = "http://127.0.0.1:1".into();
    let chosen = lib.search_choose(&row, &found, None, &opts).unwrap();
    let detail = lib.game_detail(&chosen.row_id).unwrap();
    assert_eq!(detail.row.status, RowStatus::Derivative);
    assert_eq!(
        detail.row.title,
        "Dragon Quest III - Soshite Densetsu e... (Japan) [Translation: English v2.0 Addendum]"
    );
    assert_eq!(detail.derived.unwrap().relation, Relation::Translation);
    assert!(
        !lib.search_queue(None)
            .unwrap()
            .iter()
            .any(|i| i.row_id == chosen.row_id)
    );
}

/// An IPS patch (which names no source) inside a zip beside a hack: tried
/// on your verified copy of a candidate, it gives exactly the hack, so the
/// hack is a proven patch of that game, never verified.
#[test]
fn ips_applied_to_your_verified_copy() {
    use romforge_core::suggest::Band;
    use romforge_core::view::Relation;
    use std::io::Write as _;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("roms");
    std::fs::create_dir(&dir).unwrap();
    let base = snes_rom(b"SUPER TEST QUEST     ", b"ATQE", 1, 0x10000, 0x11);
    let h = romforge_core::hash::hash_stream(&base[..], base.len() as u64)
        .unwrap()
        .raw;
    let dat = tmp.path().join("snes.dat");
    std::fs::write(
        &dat,
        format!(
            r#"<datafile><header><name>Nintendo - Super Nintendo Entertainment System</name></header>
  <game name="Super Test Quest (USA)"><rom name="Super Test Quest (USA).sfc" size="{}" crc="{}" md5="{}" sha1="{}"/></game>
</datafile>"#,
            h.size, h.crc32, h.md5, h.sha1
        ),
    )
    .unwrap();
    std::fs::write(dir.join("Super Test Quest (USA).sfc"), &base).unwrap();
    // The hack: new bytes at 0x100, and the IPS that makes it.
    let mut hack = base.clone();
    hack[0x100..0x105].copy_from_slice(b"HACKD");
    let mut ips = b"PATCH".to_vec();
    ips.extend_from_slice(&[0x00, 0x01, 0x00, 0x00, 0x05]);
    ips.extend_from_slice(b"HACKD");
    ips.extend_from_slice(b"EOF");
    {
        let mut z = zip::ZipWriter::new(std::fs::File::create(dir.join("mystery.zip")).unwrap());
        for (n, d) in [("mystery.sfc", &hack[..]), ("mystery.ips", &ips[..])] {
            z.start_file(n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(d).unwrap();
        }
        z.finish().unwrap();
    }
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.scan(&dir, Scope::All).unwrap();

    let q = lib.search_queue(None).unwrap();
    let item = q.iter().find(|i| i.title.contains("mystery")).expect("queued");
    let s = item.suggestion.clone().expect("a suggestion");
    assert_eq!(s.band, Band::PatchOf, "{s:?}");
    assert_eq!(s.title, "Super Test Quest (USA)");
    assert!(
        s.evidence
            .iter()
            .any(|e| e.kind == "patch" && e.text.contains("yields this file"))
    );
    assert_eq!(s.derived.map(|d| d.relation), Some(Relation::Hack));
    assert_eq!(item.need, Need::Identify, "a patch of a game, not that game");
}
