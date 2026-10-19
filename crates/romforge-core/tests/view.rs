//! Milestone 6: the library as the app lists it.

use std::path::{Path, PathBuf};

use romforge_core::view::RowStatus;
use romforge_core::{Library, ScanOptions, Scope};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn library() -> Library {
    let mut lib = Library::open_in_memory().unwrap();
    for e in std::fs::read_dir(fixtures().join("dats")).unwrap() {
        lib.import_dat(e.unwrap().path(), None).unwrap();
    }
    lib.scan(fixtures().join("roms"), Scope::All).unwrap();
    lib
}

#[test]
fn one_row_per_game_and_lone_files() {
    let lib = library();
    let v = lib.library_view().unwrap();
    let row = |title: &str| {
        v.rows
            .iter()
            .find(|r| r.title == title)
            .unwrap_or_else(|| panic!("{title}: {:?}", v.rows))
    };

    let gb = row("Test GB");
    assert_eq!(gb.status, RowStatus::Verified);
    assert_eq!(gb.system.as_deref(), Some("Game Boy"));
    assert_eq!(gb.files, 2, "the loose file and its zipped duplicate");
    assert_eq!(v.rows.iter().filter(|r| r.title == "Test GB").count(), 1);

    assert_eq!(row("CRC Only").status, RowStatus::Candidate);
    // One file, two revisions with identical data: both are listed.
    assert_eq!(row("Shared Hash (Rev A)").files, 1);
    assert_eq!(row("Shared Hash (Rev B)").files, 1);
    // ...as two versions of one game, one of them its main row.
    let (a, b) = (row("Shared Hash (Rev A)"), row("Shared Hash (Rev B)"));
    assert!(a.group.is_some() && a.group == b.group);
    assert!(a.main != b.main, "{a:?} {b:?}");
    assert_eq!(if a.main { a.versions } else { b.versions }, 2);
    assert!(gb.group.is_none() && gb.main && gb.versions == 1);
    let unknown = row("unknown.bin");
    assert_eq!(unknown.status, RowStatus::Unknown);
    assert_eq!(unknown.system, None);
    assert!(
        v.rows.iter().any(|r| r.title == "Test MD") && v.rows.iter().any(|r| r.title == "Test NES (in 7z)")
    );

    let names: Vec<&str> = v.platforms.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names.last(), Some(&"Unidentified"));
    let gb_platform = v
        .platforms
        .iter()
        .find(|p| p.system.as_deref() == Some("Game Boy"))
        .unwrap();
    assert_eq!(
        gb_platform.games,
        v.rows
            .iter()
            .filter(|r| r.system.as_deref() == Some("Game Boy") && r.main && !r.unmatched)
            .count(),
        "games, not versions, and only matched ones"
    );
    // What isn't matched yet is Unidentified's, whatever its platform: a
    // CRC32 alone as much as an unknown file.
    let crc = row("CRC Only");
    assert!(crc.unmatched && crc.system.as_deref() == Some("Game Boy"));
    assert!(unknown.unmatched && !gb.unmatched);
    let unidentified = v.platforms.iter().find(|p| p.system.is_none()).unwrap();
    assert_eq!(
        unidentified.games,
        v.rows.iter().filter(|r| r.main && r.unmatched).count()
    );
}

#[test]
fn detail_has_files_dats_and_links() {
    let lib = library();
    let v = lib.library_view().unwrap();
    let id = &v.rows.iter().find(|r| r.title == "Test NES (iNES)").unwrap().id;
    let d = lib.game_detail(id).unwrap();
    assert_eq!(d.files.len(), 1);
    assert_eq!(d.files[0].header_kind.as_deref(), Some("ines"));
    assert_eq!(d.dats, ["Nintendo - Nintendo Entertainment System"]);
    assert!(
        d.links
            .iter()
            .any(|l| l.url.contains("mobygames.com/search/?q=Test%20NES"))
    );

    let unknown = &v.rows.iter().find(|r| r.title == "unknown.bin").unwrap().id;
    assert!(lib.game_detail(unknown).unwrap().links.is_empty());
    assert!(lib.game_detail("g:0:nope").is_err());
}

#[test]
fn refresh_rescans_each_source_folder_once() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
    std::fs::create_dir_all(a.join("inner")).unwrap();
    std::fs::create_dir(&b).unwrap();
    std::fs::copy(fixtures().join("roms/gb/Test GB.gb"), a.join("inner/x.gb")).unwrap();
    let mut lib = library();
    lib.scan(&a, Scope::All).unwrap();
    lib.scan(a.join("inner"), Scope::All).unwrap();
    lib.scan(&b, Scope::All).unwrap();
    std::fs::remove_dir(&b).unwrap();
    std::fs::copy(fixtures().join("roms/nes/Test NES 2.0.nes"), a.join("new.nes")).unwrap();

    let results = lib.refresh(ScanOptions::default(), |_, _| {}).unwrap();
    let ok: Vec<_> = results
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .map(|r| r.root.clone())
        .collect();
    let gone: Vec<_> = results
        .iter()
        .filter_map(|r| r.as_ref().err())
        .map(|e| e.0.clone())
        .collect();
    assert!(ok.contains(&a.canonicalize().unwrap()));
    assert!(
        !ok.iter().any(|p| p.ends_with("inner")),
        "nested roots are covered by their parent"
    );
    assert_eq!(gone, [b.canonicalize().unwrap_or(b.clone())]);
    let v = lib.library_view().unwrap();
    assert!(
        v.rows.iter().any(|r| r.title == "Test NES 2.0" && r.files >= 2),
        "the new file was found"
    );
}

#[test]
fn refresh_scans_the_settings_folders() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, lib_dir) = (tmp.path().join("source"), tmp.path().join("library"));
    std::fs::create_dir(&src).unwrap();
    std::fs::create_dir(&lib_dir).unwrap();
    std::fs::copy(fixtures().join("roms/gb/Test GB.gb"), src.join("a.gb")).unwrap();
    std::fs::copy(
        fixtures().join("roms/nes/Test NES 2.0.nes"),
        lib_dir.join("b.nes"),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    for e in std::fs::read_dir(fixtures().join("dats")).unwrap() {
        lib.import_dat(e.unwrap().path(), None).unwrap();
    }
    lib.save_settings(&romforge_core::settings::Settings {
        source_dir: Some(src.clone()),
        library_dir: Some(lib_dir.clone()),
        ..Default::default()
    })
    .unwrap();
    let results = lib.refresh(ScanOptions::default(), |_, _| {}).unwrap();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 2, "{results:?}");
    let titles: Vec<String> = lib
        .library_view()
        .unwrap()
        .rows
        .into_iter()
        .map(|r| r.title)
        .collect();
    assert!(
        titles.contains(&"Test GB".to_owned()) && titles.contains(&"Test NES 2.0".to_owned()),
        "{titles:?}"
    );
}

/// Regions and translations of one game are one group; the main row
/// follows the region order in Settings.
#[test]
fn versions_group_under_the_preferred_region() {
    use romforge_core::hash::hash_stream;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("roms");
    std::fs::create_dir_all(&dir).unwrap();
    let mut roms = String::new();
    for (i, region) in ["USA", "Europe", "Japan"].iter().enumerate() {
        let data = vec![i as u8 + 10; 2048];
        let h = hash_stream(&data[..], data.len() as u64).unwrap().raw;
        roms += &format!(
            r#"<game name="Quest ({region})"><rom name="Quest ({region}).gb" size="{}" crc="{}" md5="{}" sha1="{}"/></game>"#,
            h.size, h.crc32, h.md5, h.sha1
        );
        std::fs::write(dir.join(format!("Quest ({region}).gb")), &data).unwrap();
    }
    std::fs::write(dir.join("Quest (T-En).gb"), vec![99u8; 2048]).unwrap();
    let dat = tmp.path().join("gb.dat");
    std::fs::write(
        &dat,
        format!(r#"<datafile><header><name>Nintendo - Game Boy</name></header>{roms}</datafile>"#),
    )
    .unwrap();
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(&dat, None).unwrap();
    lib.scan(&dir, Scope::All).unwrap();
    let unknown = lib
        .library_view()
        .unwrap()
        .rows
        .into_iter()
        .find(|r| r.title.contains("T-En"))
        .unwrap()
        .id;
    lib.identify_derived(
        &unknown,
        "Game Boy",
        &romforge_core::view::Derived {
            relation: romforge_core::view::Relation::Translation,
            base: "Quest (Japan)".into(),
            tag: Some("English".into()),
        },
    )
    .unwrap();

    let main = |lib: &Library| {
        let v = lib.library_view().unwrap();
        let mains: Vec<_> = v.rows.iter().filter(|r| r.main).collect();
        assert_eq!(mains.len(), 1, "one game: {:?}", v.rows);
        assert!(v.rows.iter().all(|r| r.group == mains[0].group));
        assert_eq!(v.platforms[0].games, 1);
        (mains[0].title.clone(), mains[0].versions)
    };
    assert_eq!(
        main(&lib),
        ("Quest (USA)".to_owned(), 4),
        "default order: USA first"
    );
    let japan_first = romforge_core::settings::Settings {
        regions: vec!["Japan".into(), "USA".into()],
        ..Default::default()
    };
    lib.save_settings(&japan_first).unwrap();
    assert_eq!(main(&lib), ("Quest (Japan)".to_owned(), 4));
}
