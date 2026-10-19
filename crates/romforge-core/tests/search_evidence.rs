//! Milestone 8: the Search queue on the fixtures.
//!
//! Suggestions and the evidence for them.

use romforge_core::search::Need;
use romforge_core::{Library, Scope};

mod common;
use common::fixtures;
use common::search::*;

/// Suggestions in bands, each with its evidence; none is a match.
#[test]
fn suggestions_with_evidence() {
    use romforge_core::suggest::Band;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("roms");
    std::fs::create_dir(&dir).unwrap();
    // The DAT's game: 64 KiB, serial SHVC-AQ3J-JPN.
    let original = snes_rom(b"DRAGONQUEST3         ", b"AQ3J", 0, 0x10000, 0x11);
    let h = romforge_core::hash::hash_stream(&original[..], original.len() as u64)
        .unwrap()
        .raw;
    let dat = tmp.path().join("snes.dat");
    std::fs::write(
        &dat,
        format!(
            r#"<datafile><header><name>Nintendo - Super Nintendo Entertainment System</name></header>
  <game name="Dragon Quest III - Soshite Densetsu e... (Japan)"><serial>SHVC-AQ3J-JPN</serial><rom name="dq3.sfc" size="{}" crc="{}" md5="{}" sha1="{}"/></game>
  <game name="Dragon Quest I &amp; II (Japan)"><rom name="dq12.sfc" size="1" crc="00000001"/></game>
  <game name="Super Puzzle Quest (Japan)"><rom name="spq.sfc" size="{}" crc="00000002" sha1="0000000000000000000000000000000000000002"/></game>
</datafile>"#,
            h.size, h.crc32, h.md5, h.sha1, h.size
        ),
    )
    .unwrap();
    // A translation, expanded to 96 KiB; a same-size hack; an unrelated file.
    std::fs::write(
        dir.join("Dragon Quest III (English v2.0)[Addendum].sfc"),
        snes_rom(b"DRAGONQUEST3         ", b"AQ3J", 0, 0x18000, 0x22),
    )
    .unwrap();
    std::fs::write(
        dir.join("Dragon Quest III (Hack).sfc"),
        snes_rom(b"DRAGONQUEST3         ", b"AQ3J", 0, 0x10000, 0x33),
    )
    .unwrap();
    std::fs::write(dir.join("Totally Different.sfc"), vec![7u8; 4096]).unwrap();
    // Same size, and its two words are in the DAT title: no more than that.
    std::fs::write(dir.join("Puzzle Quest.sfc"), vec![9u8; 0x10000]).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(&dir, Scope::All).unwrap();
    lib.scan(fixtures().join("roms/gb"), Scope::All).unwrap();

    let q = lib.search_queue(None).unwrap();
    let top = |needle: &str| {
        q.iter()
            .find(|i| i.title.contains(needle))
            .unwrap_or_else(|| panic!("{needle} not queued: {q:?}"))
            .suggestion
            .clone()
    };
    let tr = top("English").expect("a suggestion");
    assert_eq!(tr.band, Band::Suggested);
    assert_eq!(tr.title, "Dragon Quest III - Soshite Densetsu e... (Japan)");
    let says = |s: &romforge_core::suggest::Suggestion, kind: &str| {
        s.evidence.iter().find(|e| e.kind == kind).map(|e| e.agrees)
    };
    assert_eq!(says(&tr, "serial"), Some(true), "{:?}", tr.evidence);
    assert_eq!(says(&tr, "header_title"), Some(true));
    assert_eq!(says(&tr, "region"), Some(true));
    assert_eq!(says(&tr, "size"), Some(false));
    assert_eq!(says(&tr, "hash"), Some(false));

    let hack = top("(Hack)").expect("a suggestion");
    assert_eq!(hack.band, Band::LikelyModified);
    assert_eq!(says(&hack, "size"), Some(true));

    let crc = top("CRC Only").expect("a suggestion");
    assert_eq!((crc.band, crc.title.as_str()), (Band::LikelyCrc, "CRC Only"));
    // The preview says what a CRC32-only DAT can and can't tell.
    let row = q
        .iter()
        .find(|i| i.title.contains("CRC Only"))
        .unwrap()
        .row_id
        .clone();
    let found = lib
        .search_find(&row, romforge_core::search::Scraper::Dat, None, None, "")
        .unwrap()
        .into_iter()
        .find(|f| f.title == "CRC Only")
        .unwrap();
    let p = lib
        .search_preview(&row, &found, tmp.path(), "http://127.0.0.1:1")
        .unwrap();
    let mine = p.facts.iter().find(|(k, _)| k == "Your file").unwrap();
    assert!(mine.1.contains("same size and CRC32 as"), "{mine:?}");

    assert_eq!(top("Totally Different"), None);
    let loose = top("Puzzle Quest").expect("a suggestion");
    assert_eq!(loose.title, "Super Puzzle Quest (Japan)");
    assert!(loose.band > Band::LikelyModified, "{:?}", loose.band);

    // What you type searches; the file's own name still says translation.
    let row = q
        .iter()
        .find(|i| i.title.contains("English"))
        .unwrap()
        .row_id
        .clone();
    let typed = lib
        .search_find(
            &row,
            romforge_core::search::Scraper::Dat,
            Some("dragon quest 3"),
            None,
            "",
        )
        .unwrap();
    let dq3 = typed
        .iter()
        .find(|f| f.title.starts_with("Dragon Quest III"))
        .expect("found by what you typed");
    assert!(
        dq3.evidence.iter().any(|e| e.kind == "derived"),
        "{:?}",
        dq3.evidence
    );
    // Nothing became verified or identified.
    assert!(q.iter().all(|i| i.need == Need::Identify));
}

/// A GameCube RVZ: its header gives the game ID and the ISO's size, so it
/// is "Likely: compressed image" of the game with that serial and size,
/// never verified (that needs the whole image decompressed).
#[test]
fn compressed_images_by_id_and_size() {
    use romforge_core::suggest::Band;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("roms");
    std::fs::create_dir(&dir).unwrap();
    let iso_size = 1_459_978_240u64;
    let dat = tmp.path().join("gc.dat");
    std::fs::write(
        &dat,
        format!(
            r#"<datafile><header><name>Nintendo - GameCube</name></header>
  <game name="Super Smash Bros. Melee (USA) (En,Ja)"><serial>DL-DOL-GALE-USA</serial><rom name="Super Smash Bros. Melee (USA) (En,Ja).iso" size="{iso_size}" crc="12345678" md5="0123456789abcdef0123456789abcdef" sha1="0123456789abcdef0123456789abcdef01234567"/></game>
</datafile>"#
        ),
    )
    .unwrap();
    let mut rvz = vec![0u8; 0x1000];
    rvz[..4].copy_from_slice(b"RVZ\x01");
    rvz[0x24..0x2C].copy_from_slice(&iso_size.to_be_bytes());
    rvz[0x58..0x5E].copy_from_slice(b"GALE01");
    rvz[0x58 + 0x1C..0x58 + 0x20].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]);
    rvz[0x58 + 0x20..0x58 + 0x37].copy_from_slice(b"Super Smash Bros. Melee");
    std::fs::write(dir.join("melee.rvz"), &rvz).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, Some("GameCube")).unwrap();
    lib.scan(&dir, Scope::All).unwrap();

    let q = lib.search_queue(None).unwrap();
    let s = q[0].suggestion.clone().expect("a suggestion");
    assert_eq!(s.band, Band::LikelyCompressed, "{s:?}");
    assert_eq!(s.title, "Super Smash Bros. Melee (USA) (En,Ja)");
    assert!(
        s.evidence
            .iter()
            .any(|e| e.kind == "size" && e.agrees && e.text.starts_with("holds an image"))
    );
    assert!(
        s.evidence
            .iter()
            .any(|e| e.kind == "serial" && e.text.contains("DL-DOL-GALE-USA"))
    );
    assert_eq!(q[0].need, Need::Identify, "not verified");
}

/// An NKit image keeps the disc header (its ID) and the original image's
/// CRC32: with the serial agreeing, "Likely: compressed image".
#[test]
fn nkit_images_by_id_and_original_crc() {
    use romforge_core::suggest::Band;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("roms");
    std::fs::create_dir(&dir).unwrap();
    let dat = tmp.path().join("gc.dat");
    std::fs::write(
        &dat,
        r#"<datafile><header><name>Nintendo - GameCube</name></header>
  <game name="Super Smash Bros. Melee (USA) (En,Ja)"><serial>DL-DOL-GALE-USA</serial><rom name="Melee.iso" size="1459978240" crc="0a1b2c3d" md5="0123456789abcdef0123456789abcdef" sha1="0123456789abcdef0123456789abcdef01234567"/></game>
  <game name="Other Game (USA)"><rom name="Other.iso" size="1459978240" crc="99999999" md5="00000000000000000000000000000000" sha1="0000000000000000000000000000000000000000"/></game>
</datafile>"#,
    )
    .unwrap();
    let mut img = vec![0u8; 0x8000];
    img[..6].copy_from_slice(b"GALE01");
    img[0x1C..0x20].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]);
    img[0x20..0x37].copy_from_slice(b"Super Smash Bros. Melee");
    img[0x200..0x208].copy_from_slice(b"NKIT v01");
    img[0x208..0x20C].copy_from_slice(&0x0a1b_2c3du32.to_be_bytes());
    std::fs::write(dir.join("melee.nkit.iso"), &img).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, Some("GameCube")).unwrap();
    lib.scan(&dir, Scope::All).unwrap();
    let s = lib.search_queue(None).unwrap()[0]
        .suggestion
        .clone()
        .expect("a suggestion");
    assert_eq!(s.band, Band::LikelyCompressed, "{s:?}");
    assert_eq!(s.title, "Super Smash Bros. Melee (USA) (En,Ja)");
    assert!(s.evidence.iter().any(|e| e.kind == "nkit" && e.agrees));
}
