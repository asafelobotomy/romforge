//! End-to-end tests over the committed fixtures in `fixtures/`.
//!
//! The fixtures are tiny synthetic files, not real ROMs. They are written by
//! the ignored `regenerate_fixtures` test:
//!
//!     cargo test -p romforge-core --test fixtures -- --ignored regenerate
//!
//! DAT hashes are computed over the ROM *body* directly, never through the
//! header detector, so the tests check the detector rather than mirror it.

use std::collections::BTreeMap;
use std::path::Path;

use romforge_core::{Library, LibraryFilter, Scope, Status, systems};

mod common;
use common::fixture_data::*;
use common::fixture_lib::*;
use common::fixtures;

#[test]
#[ignore = "writes fixtures/; run explicitly"]
fn regenerate_fixtures() {
    let root = fixtures();
    let roms = root.join("roms");
    let _ = std::fs::remove_dir_all(&roms);
    let _ = std::fs::remove_dir_all(root.join("dats"));
    std::fs::create_dir_all(root.join("dats")).unwrap();

    let (fixtures, dats) = layout();
    let mut archives: BTreeMap<&str, Vec<(&str, Vec<u8>)>> = BTreeMap::new();
    let files = fixtures
        .iter()
        .map(|f| (f.path, [f.header.as_slice(), f.body.as_slice()].concat()))
        .chain(NON_ROMS.iter().map(|(p, d)| (*p, d.to_vec())));
    for (path, data) in files {
        match path.split_once('#') {
            Some((archive, member)) => archives.entry(archive).or_default().push((member, data)),
            None => {
                let p = roms.join(path);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, data).unwrap();
            }
        }
    }
    for (archive, members) in archives {
        let p = roms.join(archive);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        if archive.ends_with(".zip") {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&p).unwrap());
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .last_modified_time(zip::DateTime::default());
            for (name, data) in members {
                zip.start_file(name, opts).unwrap();
                std::io::Write::write_all(&mut zip, &data).unwrap();
            }
            zip.finish().unwrap();
        } else {
            let mut sz = sevenz_rust2::ArchiveWriter::create(&p).unwrap();
            for (name, data) in members {
                sz.push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(name), Some(&data[..]))
                    .unwrap();
            }
            sz.finish().unwrap();
        }
    }
    std::os::unix::fs::symlink(LINKED_DIR.1, roms.join(LINKED_DIR.0)).unwrap();
    for dat in &dats {
        std::fs::write(root.join("dats").join(dat.file), dat_xml(dat)).unwrap();
    }
}

#[test]
fn dats_record_source_time_and_header_hint() {
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let s = lib.summary().unwrap();
    let nes = s.dats.iter().find(|d| d.name.contains("Entertainment")).unwrap();
    assert_eq!(nes.system.as_deref(), Some("NES"));
    assert_eq!(nes.header_hint.as_deref(), Some("No-Intro_NES.xml"));
    assert!(nes.source_path.ends_with("fixtures/dats/nes.dat"));
    assert!(nes.imported_at.ends_with('Z'));
    let systems: Vec<_> = s.dats.iter().map(|d| d.system.clone().unwrap()).collect();
    for key in [
        "NES",
        "FDS",
        "Lynx",
        "Atari 7800",
        "SNES",
        "Game Boy",
        "Game Boy Advance",
        "Genesis",
    ] {
        assert!(systems.iter().any(|s| s == key), "{key} missing from {systems:?}");
    }
}

#[test]
fn identifies_every_fixture() {
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let report = lib.scan(fixtures(), Scope::All).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);

    let files = by_path(&lib);
    let (fixtures, _) = layout();
    assert_eq!(files.len(), fixtures.len(), "{:?}", files.keys());

    let expect = |path: &str, status: Status, header: Option<&str>, games: &[&str]| {
        let f = files.get(path).unwrap_or_else(|| panic!("{path} not scanned"));
        assert_eq!(f.status, status, "{path}");
        assert_eq!(f.header_kind.as_deref(), header, "{path}");
        assert_eq!(f.games, games, "{path}");
    };
    use Status::*;
    expect(
        "nes/Test NES (iNES).nes",
        Matched,
        Some("ines"),
        &["Test NES (iNES)"],
    );
    expect("nes/Test NES 2.0.nes", Matched, Some("ines"), &["Test NES 2.0"]);
    expect("fds/Test FDS.fds", Matched, Some("fds"), &["Test FDS"]);
    expect(
        "lynx/Test Lynx (headered).lnx",
        Matched,
        Some("lynx"),
        &["Test Lynx (headered)"],
    );
    expect(
        "lynx/Test Lynx (no header).lyx",
        Matched,
        None,
        &["Test Lynx (no header)"],
    );
    expect("a7800/Test 7800.a78", Matched, Some("a7800"), &["Test 7800"]);
    expect(
        "snes/Test SNES (copier).smc",
        Matched,
        Some("snes-copier"),
        &["Test SNES (copier)"],
    );
    expect(
        "snes/Test SNES (clean).sfc",
        Matched,
        None,
        &["Test SNES (clean)"],
    );
    expect("gb/CRC Only.gb", Candidate, None, &["CRC Only"]);
    expect("gb/MD5 Only.gb", Matched, None, &["MD5 Only"]);
    expect(
        "gb/Shared Hash.gb",
        Matched,
        None,
        &["Shared Hash (Rev A)", "Shared Hash (Rev B)"],
    );
    expect("unknown.bin", Unknown, None, &[]);
    expect("archives/Test GBA.zip#Test GBA.gba", Matched, None, &["Test GBA"]);
    expect("archives/Mixed.7z#Test MD.md", Matched, None, &["Test MD"]);
    expect(
        "archives/Mixed.7z#Test NES (in 7z).nes",
        Matched,
        Some("ines"),
        &["Test NES (in 7z)"],
    );
    // The same GB ROM twice: the first in path order wins, the other is a duplicate.
    expect("archives/GB copy.zip#Test GB.gb", Matched, None, &["Test GB"]);
    expect("gb/Test GB.gb", Duplicate, None, &["Test GB"]);
}

#[test]
fn have_and_missing_per_dat() {
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    lib.scan(fixtures(), Scope::All).unwrap();

    let s = lib.summary().unwrap();
    let get = |name: &str| s.dats.iter().find(|d| d.name.starts_with(name)).unwrap();
    let nes = get("Nintendo - Nintendo Entertainment System");
    assert_eq!((nes.games, nes.have, nes.missing), (4, 3, 1));
    // CRC Only is a candidate, which is not "have".
    let gb = get("Nintendo - Game Boy");
    assert_eq!((gb.games, gb.have, gb.missing), (5, 4, 1));

    let games = lib.dat_games(gb.dat_id).unwrap();
    let crc = games.iter().find(|g| g.name == "CRC Only").unwrap();
    assert!(!crc.have);
    let rev_b = games.iter().find(|g| g.name == "Shared Hash (Rev B)").unwrap();
    assert!(rev_b.have);
    assert_eq!(rev_b.files.len(), 1);
    let test_gb = games.iter().find(|g| g.name == "Test GB").unwrap();
    assert_eq!(
        test_gb.files.len(),
        2,
        "matched file and its duplicate both listed"
    );
}

#[test]
fn system_scope_marks_wrong_system_and_never_defaults() {
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let nes = systems::by_key("NES").unwrap().id;
    lib.scan(fixtures().join("roms/gb"), Scope::System(nes)).unwrap();

    let files: BTreeMap<_, _> = lib
        .files(&LibraryFilter::default())
        .unwrap()
        .into_iter()
        .map(|f| {
            (
                Path::new(&f.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                f,
            )
        })
        .collect();
    assert_eq!(files["Test GB.gb"].status, Status::WrongSystem);
    assert_eq!(files["Test GB.gb"].systems, ["Game Boy"]);
    assert_eq!(files["MD5 Only.gb"].status, Status::WrongSystem);
    // A CRC-only hit outside the scope is not evidence of anything.
    assert_eq!(files["CRC Only.gb"].status, Status::Unknown);

    let gb = systems::by_key("Game Boy").unwrap().id;
    let filtered = lib
        .files(&LibraryFilter {
            system_id: Some(gb),
            status: None,
        })
        .unwrap();
    assert_eq!(filtered.len(), 3, "Test GB, MD5 Only, Shared Hash");
}

#[test]
fn dat_scope() {
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let snes = dat_id(&lib, "Nintendo - Super Nintendo");
    lib.scan(fixtures().join("roms"), Scope::Dat(snes)).unwrap();
    let files = by_path(&lib);
    assert_eq!(files["snes/Test SNES (copier).smc"].status, Status::Matched);
    assert_eq!(files["nes/Test NES (iNES).nes"].status, Status::WrongSystem);
    assert_eq!(files["unknown.bin"].status, Status::Unknown);
}
