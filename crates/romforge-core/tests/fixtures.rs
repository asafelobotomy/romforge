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
use std::path::{Path, PathBuf};

use romforge_core::hash::hash_stream;
use romforge_core::{Library, LibraryFilter, Scope, Status, systems};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

// ---------------------------------------------------------------- generator

/// Deterministic filler bytes that never start with a header magic.
fn body(seed: &str, len: usize) -> Vec<u8> {
    let mut x: u64 = seed.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100_0000_01b3)
    });
    let mut out: Vec<u8> = (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect();
    out[0] = 0xff;
    out
}

fn header(magic_at: usize, magic: &[u8], len: usize) -> Vec<u8> {
    let mut h = vec![0u8; len];
    h[magic_at..magic_at + magic.len()].copy_from_slice(magic);
    h
}

struct Fixture {
    /// Path under fixtures/roms; `archive.zip#member` for archive members.
    path: &'static str,
    header: Vec<u8>,
    body: Vec<u8>,
}

/// One DAT game: name, body, and which hashes the DAT lists.
struct DatGame {
    name: &'static str,
    roms: Vec<(&'static str, Vec<u8>, Hashes)>,
}

#[derive(Clone, Copy)]
enum Hashes {
    All,
    Md5Crc,
    CrcOnly,
}

struct DatFile {
    file: &'static str,
    name: &'static str,
    header_hint: Option<&'static str>,
    games: Vec<DatGame>,
}

fn nes_header(nes2: bool) -> Vec<u8> {
    let mut h = header(0, b"NES\x1a", 16);
    h[4] = 1; // 16 KiB PRG units, not that anything reads it
    if nes2 {
        h[7] = 0x08;
    }
    h
}

fn layout() -> (Vec<Fixture>, Vec<DatFile>) {
    let nes1 = body("nes1", 1024);
    let nes2 = body("nes2", 1024);
    let nes7z = body("nes7z", 1024);
    let fds = body("fds", 1024);
    let lynx = body("lynx", 1024);
    let lynx_raw = body("lynx-raw", 1088);
    let a78 = body("a7800", 1024);
    let snes_copier = body("snes-copier", 2048);
    let snes_clean = body("snes-clean", 2048);
    let gb = body("gb", 1024);
    let crc_only = body("crc-only", 1024);
    let shared = body("shared", 1024);
    let md5_only = body("md5-only", 1024);
    let gba = body("gba", 1024);
    let md = body("md", 1024);

    let f = |path, header: Vec<u8>, body: &Vec<u8>| Fixture {
        path,
        header,
        body: body.clone(),
    };
    let fixtures = vec![
        f("nes/Test NES (iNES).nes", nes_header(false), &nes1),
        f("nes/Test NES 2.0.nes", nes_header(true), &nes2),
        f("fds/Test FDS.fds", header(0, b"FDS\x1a", 16), &fds),
        f("lynx/Test Lynx (headered).lnx", header(0, b"LYNX", 64), &lynx),
        f("lynx/Test Lynx (no header).lyx", vec![], &lynx_raw),
        f("a7800/Test 7800.a78", header(1, b"ATARI7800", 128), &a78),
        f("snes/Test SNES (copier).smc", vec![0; 512], &snes_copier),
        f("snes/Test SNES (clean).sfc", vec![], &snes_clean),
        f("gb/Test GB.gb", vec![], &gb),
        f("gb/CRC Only.gb", vec![], &crc_only),
        f("gb/Shared Hash.gb", vec![], &shared),
        f("gb/MD5 Only.gb", vec![], &md5_only),
        f("unknown.bin", vec![], &body("unknown", 1024)),
        f("archives/Test GBA.zip#Test GBA.gba", vec![], &gba),
        f("archives/GB copy.zip#Test GB.gb", vec![], &gb),
        f("archives/Mixed.7z#Test MD.md", vec![], &md),
        f(
            "archives/Mixed.7z#Test NES (in 7z).nes",
            nes_header(false),
            &nes7z,
        ),
    ];

    let g = |name, roms: Vec<(&'static str, &Vec<u8>, Hashes)>| DatGame {
        name,
        roms: roms.into_iter().map(|(n, b, h)| (n, b.clone(), h)).collect(),
    };
    use Hashes::*;
    let dats = vec![
        DatFile {
            file: "nes.dat",
            name: "Nintendo - Nintendo Entertainment System",
            header_hint: Some("No-Intro_NES.xml"),
            games: vec![
                g("Test NES (iNES)", vec![("Test NES (iNES).nes", &nes1, All)]),
                g("Test NES 2.0", vec![("Test NES 2.0.nes", &nes2, All)]),
                g("Test NES (in 7z)", vec![("Test NES (in 7z).nes", &nes7z, All)]),
                g(
                    "Missing NES Game",
                    vec![("Missing NES Game.nes", &body("missing", 1024), All)],
                ),
            ],
        },
        DatFile {
            file: "fds.dat",
            name: "Nintendo - Family Computer Disk System (FDS)",
            header_hint: Some("No-Intro_FDS.xml"),
            games: vec![g("Test FDS", vec![("Test FDS.fds", &fds, All)])],
        },
        DatFile {
            file: "lynx.dat",
            name: "Atari - Atari Lynx (LYX)",
            header_hint: Some("No-Intro_LNX.xml"),
            games: vec![
                g(
                    "Test Lynx (headered)",
                    vec![("Test Lynx (headered).lyx", &lynx, All)],
                ),
                g(
                    "Test Lynx (no header)",
                    vec![("Test Lynx (no header).lyx", &lynx_raw, All)],
                ),
            ],
        },
        DatFile {
            file: "a7800.dat",
            name: "Atari - Atari 7800 (BIN)",
            header_hint: Some("No-Intro_A7800.xml"),
            games: vec![g("Test 7800", vec![("Test 7800.a78", &a78, All)])],
        },
        DatFile {
            file: "snes.dat",
            name: "Nintendo - Super Nintendo Entertainment System",
            header_hint: None,
            games: vec![
                g(
                    "Test SNES (copier)",
                    vec![("Test SNES (copier).sfc", &snes_copier, All)],
                ),
                g(
                    "Test SNES (clean)",
                    vec![("Test SNES (clean).sfc", &snes_clean, All)],
                ),
            ],
        },
        DatFile {
            file: "gb.dat",
            name: "Nintendo - Game Boy",
            header_hint: None,
            games: vec![
                g("Test GB", vec![("Test GB.gb", &gb, All)]),
                g("CRC Only", vec![("CRC Only.gb", &crc_only, CrcOnly)]),
                g(
                    "Shared Hash (Rev A)",
                    vec![("Shared Hash (Rev A).gb", &shared, All)],
                ),
                g(
                    "Shared Hash (Rev B)",
                    vec![("Shared Hash (Rev B).gb", &shared, All)],
                ),
                g("MD5 Only", vec![("MD5 Only.gb", &md5_only, Md5Crc)]),
            ],
        },
        DatFile {
            file: "gba.dat",
            name: "Nintendo - Game Boy Advance",
            header_hint: None,
            games: vec![g("Test GBA", vec![("Test GBA.gba", &gba, All)])],
        },
        DatFile {
            file: "md.dat",
            name: "Sega - Mega Drive - Genesis",
            header_hint: None,
            games: vec![g("Test MD", vec![("Test MD.md", &md, All)])],
        },
    ];
    (fixtures, dats)
}

fn dat_xml(dat: &DatFile) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE datafile PUBLIC \"-//Logiqx//DTD ROM Management Datafile//EN\" \"http://www.logiqx.com/Dats/datafile.dtd\">\n<datafile>\n\t<header>\n",
    );
    s += &format!("\t\t<name>{}</name>\n", dat.name);
    s += &format!(
        "\t\t<description>{} (romforge test fixture)</description>\n",
        dat.name
    );
    s += "\t\t<version>1</version>\n\t\t<author>romforge fixtures</author>\n";
    if let Some(h) = dat.header_hint {
        s += &format!("\t\t<clrmamepro header=\"{h}\"/>\n");
    }
    s += "\t</header>\n";
    for game in &dat.games {
        s += &format!(
            "\t<game name=\"{0}\">\n\t\t<description>{0}</description>\n",
            game.name
        );
        for (name, body, which) in &game.roms {
            let h = hash_stream(&body[..], body.len() as u64).unwrap().raw;
            let hashes = match which {
                Hashes::All => format!("crc=\"{}\" md5=\"{}\" sha1=\"{}\"", h.crc32, h.md5, h.sha1),
                Hashes::Md5Crc => format!("crc=\"{}\" md5=\"{}\"", h.crc32, h.md5),
                Hashes::CrcOnly => format!("crc=\"{}\"", h.crc32),
            };
            s += &format!("\t\t<rom name=\"{name}\" size=\"{}\" {hashes}/>\n", body.len());
        }
        s += "\t</game>\n";
    }
    s + "</datafile>\n"
}

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
    for f in &fixtures {
        let data = [f.header.as_slice(), f.body.as_slice()].concat();
        match f.path.split_once('#') {
            Some((archive, member)) => archives.entry(archive).or_default().push((member, data)),
            None => {
                let p = roms.join(f.path);
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
    for dat in &dats {
        std::fs::write(root.join("dats").join(dat.file), dat_xml(dat)).unwrap();
    }
}

// -------------------------------------------------------------------- tests

fn import_all(lib: &mut Library) {
    let mut dats: Vec<_> = std::fs::read_dir(fixtures().join("dats"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    dats.sort();
    assert_eq!(dats.len(), 8);
    for d in dats {
        lib.import_dat(&d, None).unwrap();
    }
}

/// fixture path (relative to fixtures/roms, `#member` for archives) -> file
fn by_path(lib: &Library) -> BTreeMap<String, romforge_core::LibraryFile> {
    let roms = std::fs::canonicalize(fixtures().join("roms")).unwrap();
    let prefix = format!("{}/", roms.display());
    lib.files(&LibraryFilter::default())
        .unwrap()
        .into_iter()
        .map(|f| (f.display_path().strip_prefix(&prefix).unwrap().to_owned(), f))
        .collect()
}

fn dat_id(lib: &Library, name: &str) -> i64 {
    let s = lib.summary().unwrap();
    s.dats.iter().find(|d| d.name.starts_with(name)).unwrap().dat_id
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

#[test]
fn dat_without_known_system_has_none() {
    let dir = tempfile::tempdir().unwrap();
    let src = std::fs::read_to_string(fixtures().join("dats/gb.dat")).unwrap();
    let path = dir.path().join("odd.dat");
    std::fs::write(&path, src.replace("Nintendo - Game Boy", "Homebrew Things")).unwrap();

    let mut lib = Library::open_in_memory().unwrap();
    let d = lib.import_dat(&path, None).unwrap();
    assert_eq!(d.system, None);

    // Scoped to NES, the system-less DAT is out of scope: no silent fallback.
    let nes = systems::by_key("NES").unwrap().id;
    lib.scan(fixtures().join("roms/gb"), Scope::System(nes)).unwrap();
    for f in lib.files(&LibraryFilter::default()).unwrap() {
        assert_ne!(f.status, Status::Matched, "{}", f.path);
    }
    // Unscoped, it matches.
    lib.scan(fixtures().join("roms/gb"), Scope::All).unwrap();
    let matched = lib
        .files(&LibraryFilter {
            system_id: None,
            status: Some(Status::Matched),
        })
        .unwrap();
    assert_eq!(matched.len(), 3);
}

#[test]
fn reimport_and_rescan_are_idempotent() {
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let again = lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    assert!(again.unchanged);
    lib.scan(fixtures(), Scope::All).unwrap();
    let first = by_path(&lib);
    lib.scan(fixtures(), Scope::All).unwrap();
    let second = by_path(&lib);
    assert_eq!(first.len(), second.len());
    for (k, v) in &first {
        assert_eq!(v.status, second[k].status, "{k}");
    }
    assert_eq!(lib.summary().unwrap().dats.len(), 8);
}

#[test]
fn new_dat_version_replaces_old_and_keeps_scope() {
    let dir = tempfile::tempdir().unwrap();
    let src = std::fs::read_to_string(fixtures().join("dats/gb.dat")).unwrap();
    let v2 = dir.path().join("gb-v2.dat");
    std::fs::write(&v2, src.replace("<version>1</version>", "<version>2</version>")).unwrap();

    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let old = dat_id(&lib, "Nintendo - Game Boy");
    lib.scan(fixtures().join("roms/gb"), Scope::Dat(old)).unwrap();
    let new = lib.import_dat(&v2, None).unwrap();
    assert_eq!(new.replaced, Some(old));
    assert_eq!(lib.summary().unwrap().dats.len(), 8);
    let gb = lib
        .summary()
        .unwrap()
        .dats
        .into_iter()
        .find(|d| d.dat_id == new.dat_id)
        .unwrap();
    assert_eq!(gb.version.as_deref(), Some("2"));
    assert_eq!(gb.have, 4, "folder scope moved to the new DAT");
}

#[test]
fn scanning_writes_nothing_next_to_the_roms() {
    fn listing(dir: &Path) -> Vec<(PathBuf, u64)> {
        let mut v: Vec<_> = walkdir(dir);
        v.sort();
        v
    }
    fn walkdir(dir: &Path) -> Vec<(PathBuf, u64)> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                out.extend(walkdir(&p));
            } else {
                let len = p.metadata().unwrap().len();
                out.push((p, len));
            }
        }
        out
    }
    let before = listing(&fixtures());
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    lib.scan(fixtures(), Scope::All).unwrap();
    assert_eq!(before, listing(&fixtures()));
}
