use super::*;

#[test]
fn create_finds_and_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    assert_eq!(home.root(), tmp.path().join("RomForge"));
    assert!(home.games().is_dir() && home.media().is_dir() && home.trash().is_dir());
    assert_eq!(Home::at(home.root()), Some(home.clone()));
    assert_eq!(
        Home::create(home.root()).unwrap(),
        home,
        "picking the folder itself"
    );
    assert_eq!(Home::of_db(&home.db()), Some(home.clone()));

    let other = tmp.path().join("x");
    std::fs::create_dir_all(other.join("RomForge")).unwrap();
    std::fs::write(other.join("RomForge/notes.txt"), b"mine").unwrap();
    assert!(Home::create(&other).is_err(), "a stranger's RomForge folder");
}

#[test]
fn a_moved_folder_heals_its_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(&tmp.path().join("a")).unwrap();
    let game = home.games().join("Game Boy/Test GB.gb");
    std::fs::create_dir_all(game.parent().unwrap()).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/roms/gb/Test GB.gb"),
        &game,
    )
    .unwrap();
    let outside = tmp.path().join("elsewhere");
    std::fs::create_dir_all(&outside).unwrap();
    {
        let mut lib = crate::Library::open(home.db()).unwrap();
        lib.scan(home.games(), crate::Scope::All).unwrap();
        lib.scan(&outside, crate::Scope::All).unwrap();
    }
    // Moved by hand, to a place whose name starts like the old one.
    let moved = tmp.path().join("ab");
    std::fs::create_dir_all(&moved).unwrap();
    std::fs::rename(home.root(), moved.join("RomForge")).unwrap();
    let new = Home::at(&moved.join("RomForge")).unwrap();
    let lib = crate::Library::open(new.db()).unwrap();
    let paths: Vec<String> = lib
        .conn
        .prepare("SELECT path FROM scan_roots ORDER BY path")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        paths,
        [
            new.games().to_string_lossy().into_owned(),
            outside.to_string_lossy().into_owned()
        ]
    );
    let file: String = lib
        .conn
        .query_row("SELECT path FROM files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(Path::new(&file), new.games().join("Game Boy/Test GB.gb"));
    drop(lib);

    // And by romforge, to another parent.
    let third = relocate(&new, &tmp.path().join("c")).unwrap();
    assert!(!new.root().exists() && third.db().is_file());
    let lib = crate::Library::open(third.db()).unwrap();
    let file: String = lib
        .conn
        .query_row("SELECT path FROM files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(Path::new(&file), third.games().join("Game Boy/Test GB.gb"));
    assert!(
        relocate(&third, third.games().as_path()).is_err(),
        "not into itself"
    );
}

#[test]
fn adopt_moves_database_databases_and_art() {
    let tmp = tempfile::tempdir().unwrap();
    let old = tmp.path().join("old");
    let art = tmp.path().join("cache/art");
    std::fs::create_dir_all(old.join("databases")).unwrap();
    std::fs::create_dir_all(art.join("Game Boy/boxart")).unwrap();
    std::fs::write(old.join("databases/openvgdb.sqlite"), b"db").unwrap();
    let png = art.join("Game Boy/boxart/Test GB.png");
    std::fs::write(&png, b"png").unwrap();
    let db = old.join("romforge.db");
    {
        let mut lib = crate::Library::open(&db).unwrap();
        lib.import_dat(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/dats/gb.dat"),
            None,
        )
        .unwrap();
        drop(lib);
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute(
            "INSERT INTO art (system_id, title, kind, path, checked_at) VALUES (1, 'Test GB', 'boxart', ?1, '')",
            [png.to_string_lossy()],
        )
        .unwrap();
    }
    let home = Home::create(&tmp.path().join("new")).unwrap();
    let done = adopt(&home, &db, &art).unwrap();
    assert_eq!(
        done,
        Adopted {
            database: true,
            databases: 1,
            art: 1
        }
    );
    assert!(!db.exists() && !png.exists() && !art.exists());
    let moved = home.media().join("Game Boy/boxart/Test GB.png");
    assert_eq!(std::fs::read(&moved).unwrap(), b"png");
    let c = rusqlite::Connection::open(home.db()).unwrap();
    let path: String = c.query_row("SELECT path FROM art", [], |r| r.get(0)).unwrap();
    assert_eq!(Path::new(&path), moved);
    assert!(
        adopt(&home, &db, &art).is_err(),
        "a home with a database adopts nothing"
    );
}
