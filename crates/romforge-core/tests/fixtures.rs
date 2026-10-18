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
use romforge_core::{Library, LibraryFilter, ScanOptions, Scope, Status, systems};

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

/// Files a scan must skip by default (no ROM extension). `#` = archive member.
const NON_ROMS: &[(&str, &[u8])] = &[
    ("extras/Episode 01.mkv", b"\x1a\x45\xdf\xa3 not really a video"),
    ("extras/readme.nfo", b"release notes"),
    (
        "archives/Test GBA.zip#CDRomance.url",
        b"[InternetShortcut]\nURL=https://example.invalid/\n",
    ),
    ("archives/Mixed.7z#notes.txt", b"some notes"),
];

/// A symlinked folder inside the tree, pointing at a sibling. Never followed.
const LINKED_DIR: (&str, &str) = ("linked-gb", "gb");

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

#[test]
fn non_rom_files_and_members_are_skipped() {
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let report = lib.scan(fixtures().join("roms"), Scope::All).unwrap();
    assert_eq!(report.skipped, NON_ROMS.len());
    let files = by_path(&lib);
    for (path, _) in NON_ROMS {
        assert!(!files.contains_key(*path), "{path} was hashed");
    }
    assert_eq!(lib.summary().unwrap().files_by_status[2], (Status::Unknown, 1));

    // --all-files hashes them; they are unknown, everything else is unchanged.
    let opts = ScanOptions {
        all_files: true,
        ..ScanOptions::default()
    };
    let report = lib
        .scan_with(fixtures().join("roms"), Scope::All, opts, |_| {})
        .unwrap();
    assert_eq!(report.skipped, 0);
    let files = by_path(&lib);
    for (path, _) in NON_ROMS {
        assert_eq!(files[*path].status, Status::Unknown, "{path}");
    }
    assert_eq!(files["nes/Test NES (iNES).nes"].status, Status::Matched);
}

#[test]
fn nested_symlinked_folder_is_not_followed() {
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let report = lib.scan(fixtures(), Scope::All).unwrap();
    let link = std::fs::canonicalize(fixtures())
        .unwrap()
        .join("roms")
        .join(LINKED_DIR.0);
    assert_eq!(report.skipped_links, [link]);
    for f in lib.files(&LibraryFilter::default()).unwrap() {
        assert!(!f.path.contains(LINKED_DIR.0), "{} followed the link", f.path);
    }
}

#[test]
fn symlinked_root_is_followed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("my-gb");
    std::os::unix::fs::symlink(fixtures().join("roms/gb"), &root).unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let report = lib.scan(&root, Scope::All).unwrap();
    assert_eq!(report.hashed, 4);
    assert!(report.skipped_links.is_empty());
}

#[test]
fn symlink_loop_cannot_run_away() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        fixtures().join("roms/gb/Test GB.gb"),
        dir.path().join("Test GB.gb"),
    )
    .unwrap();
    std::os::unix::fs::symlink(".", dir.path().join("loop")).unwrap();
    std::os::unix::fs::symlink(dir.path(), dir.path().join("abs-loop")).unwrap();

    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let report = lib.scan(dir.path(), Scope::All).unwrap();
    assert_eq!(report.hashed, 1);
    assert_eq!(report.skipped_links.len(), 2);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}

#[test]
fn progress_reports_running_totals() {
    let mut lib = Library::open_in_memory().unwrap();
    let mut seen = Vec::new();
    let report = lib
        .scan_with(fixtures(), Scope::All, ScanOptions::default(), |p| seen.push(p))
        .unwrap();
    let last = seen.last().unwrap();
    assert_eq!(last.hashed, report.hashed);
    assert_eq!(last.bytes_hashed, report.bytes_hashed);
    assert!(seen.windows(2).all(|w| w[0].bytes_hashed <= w[1].bytes_hashed));
}

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

#[test]
fn parallel_and_serial_scans_agree() {
    let statuses = |jobs: usize| {
        let mut lib = Library::open_in_memory().unwrap();
        import_all(&mut lib);
        let opts = ScanOptions {
            jobs,
            ..ScanOptions::default()
        };
        let report = lib
            .scan_with(fixtures().join("roms"), Scope::All, opts, |_| {})
            .unwrap();
        let files: Vec<_> = by_path(&lib).into_iter().map(|(p, f)| (p, f.status)).collect();
        (report.hashed, report.bytes_hashed, report.skipped, files)
    };
    let serial = statuses(1);
    assert!(serial.0 > 10);
    assert_eq!(statuses(4), serial);
    assert_eq!(statuses(0), serial);
}

#[test]
fn disc_with_a_different_cue_sheet_counts_as_have() {
    use romforge_core::hash::hash_stream;
    let dir = tempfile::tempdir().unwrap();
    let track = vec![7u8; 4096];
    let cue = b"FILE \"Disc (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n".to_vec();
    let h = |d: &[u8]| hash_stream(d, d.len() as u64).unwrap().raw;
    let (t, c) = (h(&track), h(&cue));
    let rom = |name: &str, d: &romforge_core::hash::Digest| {
        format!(
            r#"<rom name="{name}" size="{}" crc="{}" md5="{}" sha1="{}"/>"#,
            d.size, d.crc32, d.md5, d.sha1
        )
    };
    let dat = format!(
        r#"<?xml version="1.0"?><datafile><header><name>Test - Discs</name></header>
<game name="Disc"><description>Disc</description>{}{}</game>
<game name="Sheets only">{}</game></datafile>"#,
        rom("Disc (Track 1).bin", &t),
        rom("Disc.cue", &c),
        rom("Only.cue", &c),
    );
    let dat_path = dir.path().join("discs.dat");
    std::fs::write(&dat_path, dat).unwrap();
    let roms = dir.path().join("roms");
    std::fs::create_dir(&roms).unwrap();
    std::fs::write(roms.join("Disc (Track 1).bin"), &track).unwrap();
    std::fs::write(roms.join("Disc.cue"), b"FILE \"disc.bin\" BINARY\r\n").unwrap();

    let mut lib = Library::open_in_memory().unwrap();
    let d = lib.import_dat(&dat_path, Some("PlayStation")).unwrap();
    lib.scan(&roms, Scope::Dat(d.dat_id)).unwrap();
    let games = lib.dat_games(d.dat_id).unwrap();
    let disc = games.iter().find(|g| g.name == "Disc").unwrap();
    assert!(disc.have && disc.cue_differs, "{disc:?}");
    assert_eq!((disc.roms_have, disc.roms_total), (1, 2));
    let only = games.iter().find(|g| g.name == "Sheets only").unwrap();
    assert!(!only.have, "a game of only sheets needs them to match");
    let s = lib.summary().unwrap();
    assert_eq!((s.dats[0].have, s.dats[0].missing), (1, 1));

    // The exact cue sheet makes it a plain have.
    std::fs::write(roms.join("Disc.cue"), &cue).unwrap();
    lib.scan(&roms, Scope::Dat(d.dat_id)).unwrap();
    let games = lib.dat_games(d.dat_id).unwrap();
    let disc = games.iter().find(|g| g.name == "Disc").unwrap();
    assert!(disc.have && !disc.cue_differs);
    let only = games.iter().find(|g| g.name == "Sheets only").unwrap();
    assert!(only.have, "the same sheet satisfies the sheet-only game");
}

#[test]
fn rescans_skip_unchanged_files() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("roms");
    std::fs::create_dir(&dir).unwrap();
    for f in ["gb/Test GB.gb", "nes/Test NES 2.0.nes", "archives/Test GBA.zip"] {
        let name = std::path::Path::new(f).file_name().unwrap();
        std::fs::copy(fixtures().join("roms").join(f), dir.join(name)).unwrap();
    }
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let first = lib.scan(&dir, Scope::All).unwrap();
    assert_eq!((first.hashed, first.unchanged), (3, 0));
    let matched = |lib: &Library| {
        lib.files(&LibraryFilter::default())
            .unwrap()
            .iter()
            .filter(|f| f.status == Status::Matched)
            .count()
    };
    assert_eq!(matched(&lib), 3);

    let again = lib.scan(&dir, Scope::All).unwrap();
    assert_eq!(
        (again.hashed, again.unchanged),
        (0, 3),
        "nothing read the second time"
    );
    assert_eq!(matched(&lib), 3, "and nothing lost");

    // A changed file is read again; a deleted one leaves the library.
    std::fs::write(dir.join("Test NES 2.0.nes"), b"changed").unwrap();
    std::fs::remove_file(dir.join("Test GB.gb")).unwrap();
    let after = lib.scan(&dir, Scope::All).unwrap();
    assert_eq!((after.hashed, after.unchanged), (1, 1));
    let files = lib.files(&LibraryFilter::default()).unwrap();
    assert_eq!(files.len(), 2);
    assert!(
        files
            .iter()
            .any(|f| f.path.ends_with("Test NES 2.0.nes") && f.status == Status::Unknown)
    );

    let all = lib
        .scan_with(
            &dir,
            Scope::All,
            ScanOptions {
                rehash: true,
                ..ScanOptions::default()
            },
            |_| {},
        )
        .unwrap();
    assert_eq!((all.hashed, all.unchanged), (2, 0));
}

/// N64 dumps in `.v64` / `.n64` byte order and `.smd` Mega Drive dumps
/// match the DAT's `.z64` / `.md` hashes (in temp files; the committed
/// fixtures stay as they are).
#[test]
fn other_byte_orders_match_the_dat() {
    let dir = tempfile::tempdir().unwrap();
    let mut z64 = body("z64", 32 * 1024);
    z64[..4].copy_from_slice(&[0x80, 0x37, 0x12, 0x40]);
    let md = body("md-smd", 32 * 1024);
    let v64: Vec<u8> = z64.chunks(2).flat_map(|w| [w[1], w[0]]).collect();
    let n64: Vec<u8> = z64.chunks(4).flat_map(|w| [w[3], w[2], w[1], w[0]]).collect();
    let mut smd = header(8, &[0xAA, 0xBB], 512);
    smd[1] = 3;
    for block in md.chunks(16 * 1024) {
        smd.extend(block.iter().skip(1).step_by(2));
        smd.extend(block.iter().step_by(2));
    }
    let roms = dir.path().join("roms");
    std::fs::create_dir_all(&roms).unwrap();
    std::fs::write(roms.join("Game (v64).v64"), &v64).unwrap();
    std::fs::write(roms.join("Game (n64).n64"), &n64).unwrap();
    std::fs::write(roms.join("Game.smd"), &smd).unwrap();
    let dat = |name: &str, game: &str, rom: &str, data: &[u8]| {
        let h = hash_stream(data, data.len() as u64).unwrap().raw;
        format!(
            r#"<datafile><header><name>{name}</name></header><game name="{game}"><rom name="{rom}" size="{}" crc="{}" md5="{}" sha1="{}"/></game></datafile>"#,
            h.size, h.crc32, h.md5, h.sha1
        )
    };
    std::fs::write(
        dir.path().join("n64.dat"),
        dat("Nintendo - Nintendo 64", "Game (USA)", "Game (USA).z64", &z64),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("md.dat"),
        dat(
            "Sega - Mega Drive - Genesis",
            "Game (Japan)",
            "Game (Japan).md",
            &md,
        ),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(dir.path().join("n64.dat"), None).unwrap();
    lib.import_dat(dir.path().join("md.dat"), None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    let files = lib.files(&LibraryFilter::default()).unwrap();
    assert_eq!(files.len(), 3);
    let mut matched = 0;
    for f in &files {
        match f.status {
            Status::Matched => matched += 1,
            // Both N64 files are the same game: one counts, one is a copy.
            Status::Duplicate => assert!(f.display_path().ends_with("64")),
            s => panic!("{}: {s:?}", f.display_path()),
        }
    }
    assert_eq!(matched, 2, "the N64 game and the Mega Drive one");
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
