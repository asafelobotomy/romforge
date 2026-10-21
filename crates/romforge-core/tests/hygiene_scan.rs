//! Scanning and DAT import edge cases: nested folders keep their scope,
//! a re-identify owed by a batch survives a crash, one bad zip member
//! doesn't sink its archive, nameless DATs stay apart, and paths that
//! aren't UTF-8 are reported rather than stored mangled.

use std::path::Path;

use romforge_core::{Library, LibraryFilter, ScanOptions, Scope, Status, systems};

mod common;
use common::fixtures;

fn copy_gb_roms(to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(fixtures().join("roms/gb")).unwrap() {
        let p = e.unwrap().path();
        std::fs::copy(&p, to.join(p.file_name().unwrap())).unwrap();
    }
}

fn statuses_under(lib: &Library, dir: &Path) -> Vec<Status> {
    let dir = std::fs::canonicalize(dir).unwrap();
    lib.files(&LibraryFilter::default())
        .unwrap()
        .into_iter()
        .filter(|f| Path::new(&f.path).starts_with(&dir))
        .map(|f| f.status)
        .collect()
}

/// A folder scanned with its own scope inside a folder scanned with
/// another keeps its scope through Refresh (which rescans only the outer
/// folder) and through a full rehash of the outer folder.
#[test]
fn nested_folder_keeps_its_scope() {
    let dir = tempfile::tempdir().unwrap();
    let roms = dir.path().join("roms");
    let gb = roms.join("gb");
    copy_gb_roms(&gb);
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    assert!(statuses_under(&lib, &gb).contains(&Status::Matched));

    let nes = systems::by_key("NES").unwrap().id;
    lib.scan(&gb, Scope::System(nes)).unwrap();
    let scoped = statuses_under(&lib, &gb);
    assert!(!scoped.is_empty());
    assert!(!scoped.contains(&Status::Matched), "{scoped:?}");

    let results = lib.refresh(&ScanOptions::default(), |_, _| {}).unwrap();
    assert_eq!(results.len(), 1, "only the outer folder is rescanned");
    assert_eq!(statuses_under(&lib, &gb), scoped, "after Refresh");

    let rehash = ScanOptions {
        rehash: true,
        ..ScanOptions::default()
    };
    lib.scan_with(&roms, Scope::All, &rehash, |_| {}).unwrap();
    assert_eq!(statuses_under(&lib, &gb), scoped, "after a rehash");

    // Files that went away from the inner folder still go.
    std::fs::remove_file(gb.join("Test GB.gb")).unwrap();
    lib.scan(&roms, Scope::All).unwrap();
    assert_eq!(statuses_under(&lib, &gb).len(), scoped.len() - 1);
}

/// A batch of DAT imports that stops before it identifies (here a panic,
/// as a crash would) leaves the identify owed in the database; opening
/// the library again does it.
#[test]
fn owed_identify_survives_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("lib.db");
    let roms = dir.path().join("gb");
    copy_gb_roms(&roms);
    let dat_v1 = fixtures().join("dats/gb.dat");
    let dat_v2 = dir.path().join("gb.dat");
    let mut text = std::fs::read_to_string(&dat_v1).unwrap();
    text.push_str("\n<!-- a newer copy -->\n");
    std::fs::write(&dat_v2, text).unwrap();

    let matched_with_games = |lib: &Library| {
        let files = lib
            .files(&LibraryFilter {
                system_id: None,
                status: Some(Status::Matched),
            })
            .unwrap();
        (files.len(), files.iter().all(|f| !f.games.is_empty()))
    };
    {
        let mut lib = Library::open(&db).unwrap();
        lib.import_dat(&dat_v1, None).unwrap();
        lib.scan(&roms, Scope::All).unwrap();
        assert_eq!(matched_with_games(&lib), (3, true));
        let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = lib.batch(|l| -> Result<(), romforge_core::Error> {
                // Replaces the DAT: its matches go with the old one.
                l.import_dat(&dat_v2, None)?;
                panic!("crash before the batch identifies");
            });
        }));
        assert!(crashed.is_err());
    }
    let lib = Library::open(&db).unwrap();
    assert_eq!(matched_with_games(&lib), (3, true));
}

/// A zip with one member that can't be opened (marked encrypted) still has
/// its other members hashed; the bad one is reported as an error.
#[test]
fn bad_zip_member_does_not_sink_the_archive() {
    use std::io::Write as _;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("two.zip");
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("locked.gb", stored).unwrap();
    zip.write_all(b"locked rom bytes").unwrap();
    zip.start_file("open.gb", stored).unwrap();
    zip.write_all(b"open rom bytes").unwrap();
    let mut bytes = zip.finish().unwrap().into_inner();
    // Set "encrypted" (flag bit 0) on the first member, in its local
    // header and its central directory entry.
    assert_eq!(&bytes[..4], b"PK\x03\x04");
    bytes[6] |= 1;
    let cd = bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
    bytes[cd + 8] |= 1;
    std::fs::write(&path, bytes).unwrap();

    let mut lib = Library::open_in_memory().unwrap();
    let report = lib.scan(dir.path(), Scope::All).unwrap();
    assert_eq!(report.hashed, 1, "{:?}", report.errors);
    assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
    assert_eq!(report.errors[0].1, "locked.gb");
    let members: Vec<String> = lib
        .files(&LibraryFilter::default())
        .unwrap()
        .into_iter()
        .map(|f| f.member)
        .collect();
    assert_eq!(members, ["open.gb"]);
    // The next scan reads it again, with the same result.
    let again = lib.scan(dir.path(), Scope::All).unwrap();
    assert_eq!((again.hashed, again.errors.len()), (1, 1));
}

/// Two local DATs without a name or a system are named after their files
/// and don't replace each other.
#[test]
fn nameless_dats_stay_apart() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    for (file, game) in [("first.dat", "One"), ("second.dat", "Two")] {
        let path = dir.path().join(file);
        std::fs::write(
            &path,
            format!(
                r#"<datafile><header></header><game name="{game}"><rom name="{game}.bin" size="1" crc="00000001"/></game></datafile>"#
            ),
        )
        .unwrap();
        let d = lib.import_dat(&path, None).unwrap();
        assert_eq!(d.replaced, None);
    }
    let mut names: Vec<String> = lib.summary().unwrap().dats.into_iter().map(|d| d.name).collect();
    names.sort();
    assert_eq!(names, ["first", "second"]);
}

/// A file whose name isn't UTF-8 is reported, not stored under a mangled
/// name; the files beside it are scanned as usual.
#[cfg(unix)]
#[test]
fn non_utf8_paths_are_reported() {
    use std::os::unix::ffi::OsStrExt as _;
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join(std::ffi::OsStr::from_bytes(b"bad\xff.gb"));
    std::fs::write(&bad, b"some rom bytes").unwrap();
    std::fs::write(dir.path().join("good.gb"), b"other rom bytes").unwrap();

    let mut lib = Library::open_in_memory().unwrap();
    let report = lib.scan(dir.path(), Scope::All).unwrap();
    assert_eq!(report.hashed, 1);
    assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
    assert_eq!(report.errors[0].0.file_name(), bad.file_name());
    assert!(report.errors[0].2.contains("UTF-8"));
    let paths: Vec<String> = lib
        .files(&LibraryFilter::default())
        .unwrap()
        .into_iter()
        .map(|f| f.path)
        .collect();
    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("good.gb"), "{paths:?}");
}
