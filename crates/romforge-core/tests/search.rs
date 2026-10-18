//! Milestone 8: the Search queue on the fixtures.

use std::path::{Path, PathBuf};

use romforge_core::search::Need;
use romforge_core::view::RowStatus;
use romforge_core::{Library, Scope, Status};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

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

    let s = lib.title_suggestions(&row, None, 5).unwrap();
    assert_eq!(s[0].title, "Test GB", "{s:?}");
    assert_eq!(s[0].system, "Game Boy", "only systems using .gb");
    let typed = lib.title_suggestions(&row, Some("md5 only"), 5).unwrap();
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
        romforge_core::ScanOptions {
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
    let mut lib = Library::open_in_memory().unwrap();
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
            .any(|(k, v)| k == "ROM" && v.starts_with("Test GB.gb · 1024")),
        "{p:?}"
    );
    let mine = p.facts.iter().find(|(k, _)| k == "Your file").unwrap();
    assert!(
        mine.1.contains("same size as Test GB.gb, different contents"),
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

/// A LoROM SNES image of `len` bytes with a valid header.
fn snes_rom(title: &[u8; 21], code: &[u8; 4], region: u8, len: usize, fill: u8) -> Vec<u8> {
    let mut rom = vec![fill; len];
    let h = 0x7FC0;
    rom[h..h + 21].copy_from_slice(title);
    rom[h - 0x0E..h - 0x0A].copy_from_slice(code);
    rom[h + 0x19] = region;
    rom[h + 0x1A] = 0x33;
    rom[h + 0x1C..h + 0x20].copy_from_slice(&[0x1A, 0x93, 0xE5, 0x6C]);
    rom
}

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
</datafile>"#,
            h.size, h.crc32, h.md5, h.sha1
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

    assert_eq!(top("Totally Different"), None);
    // Nothing became verified or identified.
    assert!(q.iter().all(|i| i.need == Need::Identify));
}

/// BPS's variable-length number, and a BPS patch with just its header and
/// footer (enough for romforge, which only reads those).
fn bps(source: &[u8], target: &[u8]) -> Vec<u8> {
    fn number(mut n: u64, out: &mut Vec<u8>) {
        loop {
            let x = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(0x80 | x);
                return;
            }
            out.push(x);
            n -= 1;
        }
    }
    let mut p = b"BPS1".to_vec();
    number(source.len() as u64, &mut p);
    number(target.len() as u64, &mut p);
    number(0, &mut p);
    let crc = |d: &[u8]| {
        u32::from_str_radix(
            &romforge_core::hash::hash_stream(d, d.len() as u64)
                .unwrap()
                .raw
                .crc32,
            16,
        )
        .unwrap()
    };
    p.extend_from_slice(&crc(source).to_le_bytes());
    p.extend_from_slice(&crc(target).to_le_bytes());
    let own = crc(&p);
    p.extend_from_slice(&own.to_le_bytes());
    p
}

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
            .any(|e| e.kind == "patch" && e.text.contains("turns its ROM into this file"))
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
            .any(|e| e.kind == "patch" && e.text.contains("gives exactly this file"))
    );
    assert_eq!(s.derived.map(|d| d.relation), Some(Relation::Hack));
    assert_eq!(item.need, Need::Identify, "a patch of a game, not that game");
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
