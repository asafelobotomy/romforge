//! Milestone 13: one RomForge folder. Everything romforge keeps lives in
//! one portable folder, always named `RomForge`, inside a parent folder the
//! owner picks:
//!
//! ```text
//! RomForge/
//!   .romforge                 marks the folder (and its layout version)
//!   romforge.db               the library database
//!   Games/<System>/...        imported games (zips, CHDs and sidecars)
//!   Media/<System>/<kind>/... art
//!   Databases/                OpenVGDB, LaunchBox (downloaded by you)
//!   .romforge-staging/        scratch space for imports
//!   .romforge-trash/<batch>/  originals until undone or emptied
//! ```
//!
//! A small pointer file, `$XDG_CONFIG_HOME/romforge/location`, says where
//! the folder is, so the CLI and the app find the database without a flag.
//! Staging and trash are inside, so moves stay on one drive (renames, not
//! copies).

use std::path::{Path, PathBuf};

use rusqlite::params;

use crate::Error;
use crate::organize::move_file;

/// The folder's name: always this, inside whatever parent is chosen.
pub const DIR: &str = "RomForge";
/// The marker file at the top of the folder.
const MARKER: &str = ".romforge";
const LAYOUT: &str = "1";
pub const DB: &str = "romforge.db";
/// Scratch space for imports (never scanned).
pub const STAGING: &str = ".romforge-staging";

/// A RomForge folder.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Home {
    root: PathBuf,
}

impl Home {
    /// The RomForge folder in `parent` (or `parent` itself when it is one
    /// already), created with its subfolders. An existing folder named
    /// RomForge without the marker is used only when empty.
    pub fn create(parent: &Path) -> Result<Home, Error> {
        let parent = std::path::absolute(parent)?;
        let root = if parent.file_name().is_some_and(|n| n == DIR) {
            parent
        } else {
            parent.join(DIR)
        };
        if root.exists() && !root.join(MARKER).exists() && std::fs::read_dir(&root)?.next().is_some() {
            return Err(Error::Settings(format!(
                "{} exists and isn't a RomForge folder; pick another place or empty it",
                root.display()
            )));
        }
        let home = Home { root };
        for dir in [
            home.games(),
            home.media(),
            home.databases(),
            home.staging(),
            home.trash(),
        ] {
            std::fs::create_dir_all(dir)?;
        }
        if !home.root.join(MARKER).exists() {
            std::fs::write(home.root.join(MARKER), format!("romforge layout {LAYOUT}\n"))?;
        }
        Ok(home)
    }

    /// `dir` if it is a RomForge folder (has the marker).
    pub fn at(dir: &Path) -> Option<Home> {
        dir.join(MARKER).is_file().then(|| Home { root: dir.to_owned() })
    }

    /// The folder holding the database at `db`, if that is a RomForge folder.
    pub fn of_db(db: &Path) -> Option<Home> {
        Home::at(std::path::absolute(db).ok()?.parent()?)
    }

    /// The folder the pointer file names, if it is still there.
    pub fn locate() -> Option<Home> {
        if let Some(p) = std::env::var_os("ROMFORGE_HOME") {
            return Home::at(Path::new(&p));
        }
        let text = std::fs::read_to_string(pointer_file()?).ok()?;
        Home::at(Path::new(text.trim()))
    }

    /// Record this folder as the one to use.
    pub fn remember(&self) -> Result<(), Error> {
        let Some(file) = pointer_file() else {
            return Err(Error::Settings("no config folder ($HOME is not set)".into()));
        };
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(file, format!("{}\n", self.root.display()))?;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn db(&self) -> PathBuf {
        self.root.join(DB)
    }
    pub fn games(&self) -> PathBuf {
        self.root.join("Games")
    }
    pub fn media(&self) -> PathBuf {
        self.root.join("Media")
    }
    pub fn databases(&self) -> PathBuf {
        self.root.join("Databases")
    }
    pub fn staging(&self) -> PathBuf {
        self.root.join(STAGING)
    }
    pub fn trash(&self) -> PathBuf {
        self.root.join(crate::cleanup::TRASH_DIR)
    }

    /// Whether `path` is inside this folder.
    pub fn contains(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }
}

/// `$XDG_CONFIG_HOME/romforge/location`, else `~/.config/romforge/location`.
fn pointer_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("romforge").join("location"))
}

/// Where the database last saw its folder (a `settings` row).
const ROOT_KEY: &str = "home_root";

/// Every stored path, by table and column: rewritten when the folder moves.
const PATH_COLUMNS: &[(&str, &str)] = &[
    ("files", "path"),
    ("folder_games", "path"),
    ("scummvm_ids", "path"),
    ("scan_roots", "path"),
    ("art", "path"),
    ("dats", "source_path"),
    ("cleanup_outputs", "path"),
    ("cleanup_trash", "original"),
    ("cleanup_trash", "trashed"),
    ("organize_batches", "dest"),
    ("organize_moves", "src"),
    ("organize_moves", "dst"),
    ("settings", "value"),
];

/// After the folder moved (by hand, or by [`relocate`]): rewrite every
/// stored path inside its old place to the new one. Returns whether
/// anything had to change. Paths outside the folder stay as they are.
pub(crate) fn heal(conn: &mut rusqlite::Connection, root: &Path) -> Result<bool, Error> {
    use rusqlite::OptionalExtension;
    let now = root.to_string_lossy().into_owned();
    let was: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [ROOT_KEY], |r| {
            r.get(0)
        })
        .optional()?;
    let moved = was.as_ref().is_some_and(|w| *w != now);
    let tx = conn.transaction()?;
    if let Some(old) = was.as_ref().filter(|_| moved) {
        for (table, col) in PATH_COLUMNS {
            tx.execute(
                &format!(
                    "UPDATE {table} SET {col} = ?2 || substr({col}, length(?1) + 1)
                     WHERE {col} = ?1 OR substr({col}, 1, length(?1) + 1) = ?1 || '/'"
                ),
                params![old, now],
            )?;
        }
    }
    if moved || was.is_none() {
        tx.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![ROOT_KEY, now],
        )?;
    }
    tx.commit()?;
    Ok(moved)
}

/// Move the whole folder into `new_parent` (as `new_parent/RomForge`),
/// with no connection open on its database: one rename on the same drive,
/// else file by file, each copy checked before its original goes. Nothing
/// is ever replaced. The caller records the new place ([`Home::remember`]);
/// the database fixes its stored paths the next time it opens ([`heal`]).
pub fn relocate(home: &Home, new_parent: &Path) -> Result<Home, Error> {
    let parent = std::path::absolute(new_parent)?;
    let to = if parent.file_name().is_some_and(|n| n == DIR) {
        parent
    } else {
        parent.join(DIR)
    };
    if to.starts_with(&home.root) {
        return Err(Error::Settings(
            "the new place is inside the RomForge folder".into(),
        ));
    }
    if to.exists() {
        return Err(Error::Settings(format!("{} already exists", to.display())));
    }
    if let Some(p) = to.parent() {
        std::fs::create_dir_all(p)?;
    }
    match std::fs::rename(&home.root, &to) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            move_tree(&home.root, &to)?;
            let _ = std::fs::remove_dir(&home.root);
        }
        Err(e) => return Err(e.into()),
    }
    let new = Home::at(&to)
        .ok_or_else(|| Error::Settings(format!("{} has no marker after the move", to.display())))?;
    Ok(new)
}

/// What [`adopt`] moved into the folder.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Adopted {
    pub database: bool,
    pub databases: usize,
    pub art: usize,
}

/// Move an existing setup into `home`: the database at `old_db` (with no
/// connection open on it), the downloaded databases beside it, and the art
/// under `old_art`. Moves never replace a file and are checked when they
/// cross drives; art paths in the database are rewritten to match. Nothing
/// is moved if `home` already has a database.
pub fn adopt(home: &Home, old_db: &Path, old_art: &Path) -> Result<Adopted, Error> {
    let mut done = Adopted::default();
    if home.db().exists() {
        return Err(Error::Settings(format!(
            "{} already has a database",
            home.root.display()
        )));
    }
    if old_db.exists() {
        // Fold the write-ahead log into the file so it moves whole.
        {
            let c = rusqlite::Connection::open(old_db)?;
            c.pragma_update(None, "journal_mode", "DELETE")?;
        }
        move_file(old_db, &home.db())?;
        done.database = true;
        let old_ext = crate::extdb::dir_for(old_db);
        if old_ext.is_dir() {
            done.databases = move_tree(&old_ext, &home.databases())?;
        }
    }
    if old_art.is_dir() {
        done.art = move_tree(old_art, &home.media())?;
        if done.database {
            let c = rusqlite::Connection::open(home.db())?;
            let (old, new) = (
                format!("{}/", old_art.display()),
                format!("{}/", home.media().display()),
            );
            c.execute(
                "UPDATE art SET path = ?2 || substr(path, length(?1) + 1)
                 WHERE substr(path, 1, length(?1)) = ?1",
                params![old, new],
            )?;
        }
    }
    Ok(done)
}

/// Move every file under `from` to the same place under `to`, then remove
/// the folders left empty. Returns the number of files moved.
pub(crate) fn move_tree(from: &Path, to: &Path) -> Result<usize, Error> {
    let mut n = 0;
    for e in walkdir::WalkDir::new(from).follow_links(false) {
        let e = e.map_err(std::io::Error::from)?;
        if !e.file_type().is_file() {
            continue;
        }
        let rel = e.path().strip_prefix(from).unwrap_or(e.path());
        move_file(e.path(), &to.join(rel))?;
        n += 1;
    }
    for e in walkdir::WalkDir::new(from).contents_first(true) {
        if let Ok(e) = e
            && e.file_type().is_dir()
        {
            let _ = std::fs::remove_dir(e.path());
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
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
}
