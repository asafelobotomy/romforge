use super::*;

#[test]
fn create_finds_and_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(tmp.path()).unwrap();
    assert_eq!(home.root(), tmp.path().join("RomForge"));
    assert!(home.games().is_dir() && home.media().is_dir() && home.trash().is_dir());
    assert!(home.bios().is_dir() && home.saves().is_dir() && home.emulators().is_dir());
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

fn tree(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = walkdir::WalkDir::new(dir)
        .min_depth(1)
        .into_iter()
        .map(|e| {
            let e = e.unwrap();
            let rel = e.path().strip_prefix(dir).unwrap().display().to_string();
            if e.file_type().is_dir() {
                format!("{rel}/")
            } else {
                rel
            }
        })
        .collect();
    v.sort();
    v
}

/// A folder moved where a rename can't is copied whole (empty folders
/// too) beside its new place, then renamed in, and only then removed.
#[test]
fn move_tree_is_all_or_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let from = tmp.path().join("game");
    std::fs::create_dir_all(from.join("saves")).unwrap();
    std::fs::create_dir_all(from.join("data/sub")).unwrap();
    std::fs::write(from.join("a.bin"), vec![1u8; 70_000]).unwrap();
    std::fs::write(from.join("data/sub/b.bin"), b"b").unwrap();
    let before = tree(&from);
    let to = tmp.path().join("trash/game");
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    assert_eq!(move_tree(&from, &to).unwrap(), 2);
    assert!(!from.exists());
    assert_eq!(tree(&to), before, "empty folders kept");
    assert_eq!(tree(to.parent().unwrap()), {
        let mut v: Vec<String> = before.iter().map(|p| format!("game/{p}")).collect();
        v.push("game/".into());
        v.sort();
        v
    });
    assert!(move_tree(&to, &to).is_err(), "never onto something there");

    // A file that can't be read: nothing moves, nothing is left beside.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let locked = to.join("data/sub/b.bin");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&locked).is_err() {
            let back = tmp.path().join("back");
            assert!(move_tree(&to, &back).is_err());
            assert!(!back.exists());
            assert_eq!(
                tree(tmp.path())
                    .iter()
                    .filter(|p| p.contains("romforge-part"))
                    .count(),
                0
            );
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert_eq!(tree(&to), before, "the source is untouched");
        }
    }
}

/// A move cut short (the copy in place, some of the source left) is
/// finished by merging: the same file goes, a missing one moves, one that
/// differs stays and is an error.
#[test]
fn merge_tree_finishes_a_move() {
    let tmp = tempfile::tempdir().unwrap();
    let (from, to) = (tmp.path().join("from"), tmp.path().join("to"));
    std::fs::create_dir_all(from.join("empty")).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    std::fs::write(from.join("same"), b"same").unwrap();
    std::fs::write(to.join("same"), b"same").unwrap();
    std::fs::write(from.join("new"), b"new").unwrap();
    std::fs::write(from.join("odd"), b"from").unwrap();
    std::fs::write(to.join("odd"), b"to").unwrap();
    assert!(merge_tree(&from, &to).is_err());
    assert_eq!(tree(&from), ["odd"]);
    assert_eq!(tree(&to), ["empty/", "new", "odd", "same"]);
    assert_eq!(std::fs::read(to.join("odd")).unwrap(), b"to", "never replaced");
    std::fs::remove_file(to.join("odd")).unwrap();
    merge_tree(&from, &to).unwrap();
    assert!(!from.exists());
}

/// One rule for creating the folder, the CLI's and the app's: a folder in
/// use already is the only one accepted.
#[test]
fn create_or_adopt_refuses_a_second_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let old_db = tmp.path().join("old/romforge.db");
    std::fs::create_dir_all(old_db.parent().unwrap()).unwrap();
    crate::Library::open(&old_db).unwrap();
    let art = tmp.path().join("art");
    let done = create_or_adopt(&tmp.path().join("a"), None, &old_db, &art).unwrap();
    let home = done.home().clone();
    assert!(matches!(done, Created::Adopted { .. }) && home.db().is_file() && !old_db.exists());
    let again = create_or_adopt(home.root(), Some(&home), &home.db(), &art).unwrap();
    assert_eq!(again, Created::Existing { home: home.clone() });
    let e = create_or_adopt(&tmp.path().join("b"), Some(&home), &home.db(), &art).unwrap_err();
    assert!(e.to_string().contains("already using"), "{e}");
    assert!(!tmp.path().join("b").exists(), "nothing made");
    let fresh = create_or_adopt(&tmp.path().join("c"), None, &tmp.path().join("none.db"), &art).unwrap();
    assert!(matches!(fresh, Created::New { .. }));
}

/// Patch files and Wikidata pictures follow the folder too.
#[test]
fn every_stored_path_heals() {
    let tmp = tempfile::tempdir().unwrap();
    let home = Home::create(&tmp.path().join("a")).unwrap();
    let (patch, pic) = (home.games().join("x.ips"), home.media().join("q.png"));
    {
        let lib = crate::Library::open(home.db()).unwrap();
        lib.conn
            .execute(
                "INSERT INTO scan_roots (path, scanned_at) VALUES (?1, '')",
                [home.games().to_string_lossy()],
            )
            .unwrap();
        lib.conn
            .execute(
                "INSERT INTO patch_files (path, root_id, name, kind, sha1, size, hint, relation)
                 VALUES (?1, (SELECT max(root_id) FROM scan_roots), 'x.ips', 'IPS', '', 0, '', 'hack')",
                [patch.to_string_lossy()],
            )
            .unwrap();
        lib.conn
            .execute(
                "INSERT INTO wikidata_pictures (qid, path, checked_at) VALUES ('Q1', ?1, '')",
                [pic.to_string_lossy()],
            )
            .unwrap();
    }
    let new = relocate(&home, &tmp.path().join("b")).unwrap();
    let lib = crate::Library::open(new.db()).unwrap();
    let get = |sql: &str| -> String { lib.conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(
        Path::new(&get("SELECT path FROM patch_files")),
        new.games().join("x.ips")
    );
    assert_eq!(
        Path::new(&get("SELECT path FROM wikidata_pictures")),
        new.media().join("q.png")
    );
}

/// Every column that holds a path is rewritten when the folder moves: a
/// new table with a `path` (or similar) column must be listed too.
#[test]
fn every_path_column_heals() {
    let conn = crate::db::open_in_memory().unwrap();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let path_like = |c: &str| {
        c == "path"
            || c.ends_with("_path")
            || ["backup", "original", "trashed", "dest", "src", "dst"].contains(&c)
    };
    for t in tables {
        let cols: Vec<String> = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{t}')"))
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for c in cols.iter().filter(|c| path_like(c)) {
            assert!(
                PATH_COLUMNS.contains(&(t.as_str(), c.as_str())),
                "{t}.{c} is not in PATH_COLUMNS"
            );
        }
    }
}
