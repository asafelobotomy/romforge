//! End-to-end tests over the committed fixtures in `fixtures/`.
//!
//! The fixtures are tiny synthetic files, not real ROMs. They are written by
//! the ignored `regenerate_fixtures` test:
//!
//!     cargo test -p romforge-core --test fixtures -- --ignored regenerate
//!
//! DAT hashes are computed over the ROM *body* directly, never through the
//! header detector, so the tests check the detector rather than mirror it.
//!
//! Importing and tracking DATs.

use romforge_core::{Library, LibraryFilter, Scope, Status};

mod common;
use common::fixture_lib::*;
use common::fixtures;

#[test]
fn zip_of_dats_imports_each_dat_from_memory() {
    let dir = tempfile::tempdir().unwrap();
    let pack = dir.path().join("pack.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&pack).unwrap());
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, file) in [
        ("b/nes.dat", "nes.dat"),
        ("a/gb.dat", "gb.dat"),
        ("readme.txt", "md.dat"),
    ] {
        zip.start_file(name, opts).unwrap();
        let data = std::fs::read(fixtures().join("dats").join(file)).unwrap();
        std::io::Write::write_all(&mut zip, &data).unwrap();
    }
    zip.finish().unwrap();
    let before: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();

    let mut lib = Library::open_in_memory().unwrap();
    let got = lib.import_dats(&pack, None).unwrap();
    let systems: Vec<_> = got.iter().map(|d| d.system).collect();
    assert_eq!(systems, [Some("Game Boy"), Some("NES")], "sorted, .txt ignored");
    let s = lib.summary().unwrap();
    assert!(s.dats.iter().all(|d| d.source_path.contains("pack.zip#")));
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        before.len(),
        "nothing extracted"
    );

    assert!(
        lib.import_dats(&pack, Some("NES")).is_err(),
        "one --system for many DATs"
    );
    assert!(lib.import_dats(fixtures().join("dats/gb.dat"), None).unwrap()[0].unchanged);
}

#[test]
fn nointro_status_compares_versions_by_dat_name() {
    use romforge_core::DatState;
    use romforge_core::fetch::ProfileEntry;

    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let gb = lib
        .summary()
        .unwrap()
        .dats
        .into_iter()
        .find(|d| d.name == "Nintendo - Game Boy")
        .unwrap();
    let nes = lib
        .summary()
        .unwrap()
        .dats
        .into_iter()
        .find(|d| d.name.contains("Entertainment"))
        .unwrap();
    let entry = |name: &str, version: &str| ProfileEntry {
        name: name.into(),
        version: version.into(),
    };
    let profile = [
        entry(&gb.name, gb.version.as_deref().unwrap()),
        entry(&nes.name, "29990101-000000"),
        entry("Nintendo - Virtual Boy", "20250101-000000"),
        entry("Some - Unknown Thing", "1"),
    ];
    let st = lib.nointro_status(&profile).unwrap();
    let states: Vec<_> = st.iter().map(|s| (s.system, s.state)).collect();
    assert_eq!(
        states,
        [
            ("Game Boy", DatState::Current),
            ("NES", DatState::Outdated),
            ("Virtual Boy", DatState::Missing),
        ]
    );
    assert_eq!(st[1].dat_id, Some(nes.dat_id));
}

#[test]
fn download_refuses_systems_without_a_source() {
    let mut lib = Library::open_in_memory().unwrap();
    let mut called = false;
    let err = lib
        .download_dats("Arcade", true, |_, _| called = true)
        .unwrap_err()
        .to_string();
    assert!(err.contains("no DAT source"), "{err}");
    assert!(lib.download_dats("nope", true, |_, _| called = true).is_err());
    assert!(!called, "nothing fetched");
}

#[test]
fn clrmamepro_dat_imports_and_matches() {
    // The gb fixture DAT rewritten in clrmamepro text, as libretro and
    // Redump BIOS DATs come.
    let xml = std::fs::read(fixtures().join("dats/gb.dat")).unwrap();
    let dat = romforge_core::dat::parse_any(&xml).unwrap();
    let mut cmp = format!("clrmamepro (\n\tname \"{}\"\n\tversion 2026.10.07\n)\n", dat.name);
    for g in &dat.games {
        cmp += &format!("game (\n\tname \"{}\"\n", g.name);
        for r in &g.roms {
            cmp += &format!("\trom ( name \"{}\" size {}", r.name, r.size.unwrap());
            for (k, v) in [("crc", &r.crc32), ("md5", &r.md5), ("sha1", &r.sha1)] {
                if let Some(v) = v {
                    cmp += &format!(" {k} {}", v.to_ascii_uppercase());
                }
            }
            cmp += " )\n";
        }
        cmp += ")\n";
    }
    let mut lib = Library::open_in_memory().unwrap();
    let d = lib
        .import_dat_bytes(cmp.as_bytes(), "test", Some("Game Boy"))
        .unwrap();
    assert_eq!(d.games, dat.games.len());
    lib.scan(fixtures().join("roms/gb"), Scope::Dat(d.dat_id))
        .unwrap();
    let have = lib.summary().unwrap().dats[0].have;
    assert_eq!(have, 4);
}

#[test]
fn mirror_dat_counts_for_the_no_intro_main_set() {
    use romforge_core::DatState;
    use romforge_core::fetch::{ProfileEntry, Source};

    let mut lib = Library::open_in_memory().unwrap();
    let xml = std::fs::read(fixtures().join("dats/gb.dat")).unwrap();
    let url = Source::NoIntro("Nintendo - Game Boy").url().unwrap();
    lib.import_dat_bytes(&xml, &url, Some("Game Boy")).unwrap();
    let profile = [
        ProfileEntry {
            name: "Nintendo - Game Boy".into(),
            version: "20261001-000000".into(),
        },
        ProfileEntry {
            name: "Nintendo - Game Boy (Private)".into(),
            version: "20261001-000000".into(),
        },
    ];
    let st: Vec<_> = lib
        .nointro_status(&profile)
        .unwrap()
        .into_iter()
        .map(|s| s.state)
        .collect();
    assert_eq!(st, [DatState::Mirror, DatState::Missing]);
}

#[test]
fn same_name_from_another_source_is_kept() {
    let xml = std::fs::read_to_string(fixtures().join("dats/gb.dat")).unwrap();
    let v2 = xml.replace("<version>1</version>", "<version>2</version>");
    let v3 = xml.replace("<version>1</version>", "<version>3</version>");
    let mut lib = Library::open_in_memory().unwrap();
    let a = lib
        .import_dat_bytes(xml.as_bytes(), "https://a.example/gb.dat", None)
        .unwrap();
    let b = lib
        .import_dat_bytes(v2.as_bytes(), "https://b.example/gb.dat", None)
        .unwrap();
    assert_eq!(b.replaced, None, "different URL, both kept");
    let a2 = lib
        .import_dat_bytes(v3.as_bytes(), "https://a.example/gb.dat", None)
        .unwrap();
    assert_eq!(a2.replaced, Some(a.dat_id), "same URL, newer version replaces");
    assert_eq!(lib.summary().unwrap().dats.len(), 2);
}

#[test]
fn newer_tosec_release_replaces_older_one() {
    let xml = std::fs::read_to_string(fixtures().join("dats/gb.dat")).unwrap();
    let v2 = xml.replace("<version>1</version>", "<version>2</version>");
    let mut lib = Library::open_in_memory().unwrap();
    let old = "https://www.tosecdev.org/downloads/category/58-2024-03-31?download=1#TOSEC/Nintendo Game Boy - Games (TOSEC-v2024).dat";
    let new = "https://www.tosecdev.org/downloads/category/59-2025-03-13?download=2#TOSEC/Nintendo Game Boy - Games (TOSEC-v2025).dat";
    let a = lib.import_dat_bytes(xml.as_bytes(), old, None).unwrap();
    let b = lib.import_dat_bytes(v2.as_bytes(), new, None).unwrap();
    assert_eq!(b.replaced, Some(a.dat_id));
    let file = lib.import_dat_bytes(xml.as_bytes(), "/tmp/gb.dat", None).unwrap();
    assert_eq!(file.replaced, None, "a hand import never replaces a download");
    assert_eq!(lib.summary().unwrap().dats.len(), 2);
}

#[test]
fn batch_identifies_once_at_the_end() {
    let mut lib = Library::open_in_memory().unwrap();
    lib.scan(fixtures().join("roms/gb"), Scope::All).unwrap();
    let matched = |lib: &Library| {
        lib.files(&LibraryFilter::default())
            .unwrap()
            .iter()
            .filter(|f| f.status == Status::Matched)
            .count()
    };
    assert_eq!(matched(&lib), 0);
    lib.batch(|lib| -> Result<(), romforge_core::Error> {
        lib.batch(|lib| lib.import_dat(fixtures().join("dats/gb.dat"), None).map(drop))?;
        assert_eq!(matched(lib), 0, "nested batch defers too");
        Ok(())
    })
    .unwrap();
    assert!(matched(&lib) > 0, "identified when the outer batch ended");

    let mut lib = Library::open_in_memory().unwrap();
    lib.scan(fixtures().join("roms/gb"), Scope::All).unwrap();
    let err = lib.batch(|lib| -> Result<(), romforge_core::Error> {
        lib.import_dat(fixtures().join("dats/gb.dat"), None)?;
        lib.import_dat(fixtures().join("nope.dat"), None)?;
        Ok(())
    });
    assert!(err.is_err());
    assert!(matched(&lib) > 0, "DATs imported before the failure still count");
}

/// Serials from a DAT and from a libretro serial list (matched to the DAT
/// game by CRC when the names differ); they outlive the DAT's re-import.
#[test]
fn serials_from_dats_and_libretro_lists() {
    let dir = tempfile::tempdir().unwrap();
    let psx = dir.path().join("psx.dat");
    std::fs::write(
        &psx,
        r#"<datafile><header><name>Sony - PlayStation</name></header>
  <game name="Final Fantasy VII (USA) (Disc 1)"><category>Games</category><serial>SCUS-94163</serial><rom name="a.bin" size="1" crc="00000001"/></game>
  <game name="Final Fantasy VII (USA) (Disc 2)"><serial>SCUS-94163</serial><rom name="b.bin" size="1" crc="00000002"/></game>
</datafile>"#,
    )
    .unwrap();
    let snes = dir.path().join("snes.dat");
    std::fs::write(
        &snes,
        r#"<datafile><header><name>Nintendo - Super Nintendo Entertainment System</name></header>
  <game name="Dragon Quest III - Soshite Densetsu e... (Japan)"><rom name="x.sfc" size="4194304" crc="13836bd2"/></game>
</datafile>"#,
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&psx, None).unwrap();
    lib.import_dat(&snes, None).unwrap();
    assert_eq!(
        lib.titles_by_serial("PlayStation", "SCUS_941.63").unwrap(),
        [
            "Final Fantasy VII (USA) (Disc 1)",
            "Final Fantasy VII (USA) (Disc 2)"
        ],
        "a serial narrows, here to two discs"
    );
    let list = b"clrmamepro ( name \"x\" )\ngame (\n\tcomment \"Dragon Quest III (Japan)\"\n\tserial \"SHVC-AQ3J-JPN\"\n\trom ( crc 13836BD2 )\n)\n";
    assert_eq!(
        lib.store_serials("SNES", list).unwrap(),
        2,
        "its own title and the DAT's"
    );
    assert_eq!(
        lib.titles_by_serial("SNES", "SHVC-AQ3J-JPN").unwrap(),
        [
            "Dragon Quest III (Japan)",
            "Dragon Quest III - Soshite Densetsu e... (Japan)"
        ]
    );
    // A new version of the DAT replaces the old; the serials stay.
    std::fs::write(
        &snes,
        std::fs::read_to_string(&snes)
            .unwrap()
            .replace("</datafile>", "<!-- v2 --></datafile>"),
    )
    .unwrap();
    assert!(lib.import_dat(&snes, None).unwrap().replaced.is_some());
    assert_eq!(lib.titles_by_serial("SNES", "SHVCAQ3JJPN").unwrap().len(), 2);
}
