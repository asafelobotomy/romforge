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

use crate::layout::{DATABASES_DIR, DB_FILE, GAMES_DIR, HOME_DIR as DIR, HOME_MARKER as MARKER, MEDIA_DIR};

/// The layout version the marker file records.
const LAYOUT: &str = "1";

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
        self.root.join(DB_FILE)
    }
    pub fn games(&self) -> PathBuf {
        self.root.join(GAMES_DIR)
    }
    pub fn media(&self) -> PathBuf {
        self.root.join(MEDIA_DIR)
    }
    pub fn databases(&self) -> PathBuf {
        self.root.join(DATABASES_DIR)
    }
    pub fn staging(&self) -> PathBuf {
        self.root.join(crate::layout::STAGING_DIR)
    }
    pub fn trash(&self) -> PathBuf {
        self.root.join(crate::layout::TRASH_DIR)
    }

    /// Whether `path` is inside this folder.
    pub fn contains(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }
}

/// `$XDG_CONFIG_HOME/romforge/location`, else `~/.config/romforge/location`.
fn pointer_file() -> Option<PathBuf> {
    Some(crate::layout::xdg("XDG_CONFIG_HOME", ".config")?.join(crate::layout::POINTER_FILE))
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
mod tests;
