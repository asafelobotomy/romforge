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
//!   Emulators/                emulators and cores (AppImages, `cores/`);
//!                             `config/<emulator>/`: romforge's copy of its config
//!   Bios/                     BIOS files (`Bios/RetroArch/`: its system folder)
//!   Saves/ States/ Screenshots/   what games save, per system
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
use crate::organize::{move_file, rename_noreplace};

use crate::layout::{
    BIOS_DIR, DATABASES_DIR, DB_FILE, EMULATOR_CONFIG_DIR, EMULATORS_DIR, GAMES_DIR, HOME_DIR as DIR,
    HOME_MARKER as MARKER, MEDIA_DIR, SAVES_DIR, SCREENSHOTS_DIR, STATES_DIR,
};

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
        let root = root_in(parent)?;
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
            home.emulators(),
            home.bios(),
            home.saves(),
            home.states(),
            home.screenshots(),
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

    /// The RomForge folder [`Home::create`] makes for `parent`, if it is
    /// there.
    pub fn in_parent(parent: &Path) -> Option<Home> {
        Home::at(&root_in(parent).ok()?)
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
    pub fn emulators(&self) -> PathBuf {
        self.root.join(EMULATORS_DIR)
    }
    /// romforge's own copy of `emulator`'s config.
    pub fn emulator_config(&self, emulator: &str) -> PathBuf {
        self.emulators().join(EMULATOR_CONFIG_DIR).join(emulator)
    }
    pub fn bios(&self) -> PathBuf {
        self.root.join(BIOS_DIR)
    }
    pub fn saves(&self) -> PathBuf {
        self.root.join(SAVES_DIR)
    }
    pub fn states(&self) -> PathBuf {
        self.root.join(STATES_DIR)
    }
    pub fn screenshots(&self) -> PathBuf {
        self.root.join(SCREENSHOTS_DIR)
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

/// The RomForge folder for a parent the owner picked: `parent/RomForge`, or
/// `parent` itself when it is named RomForge.
fn root_in(parent: &Path) -> Result<PathBuf, Error> {
    let parent = std::path::absolute(parent)?;
    Ok(if parent.file_name().is_some_and(|n| n == DIR) {
        parent
    } else {
        parent.join(DIR)
    })
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
    ("patch_files", "path"),
    ("wikidata_pictures", "path"),
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
/// else copied whole beside its new place, checked, renamed into place, and
/// only then removed from the old one (`move_tree`). Nothing is ever
/// replaced. The caller records the new place ([`Home::remember`]); the
/// database fixes its stored paths the next time it opens (`heal`).
pub fn relocate(home: &Home, new_parent: &Path) -> Result<Home, Error> {
    let to = root_in(new_parent)?;
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
    match rename_noreplace(&home.root, &to) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            // Once the copy is in place the folder has moved, even if some
            // of the old one couldn't be removed (that is left as it is).
            if let Err(e) = move_tree(&home.root, &to)
                && Home::at(&to).is_none()
            {
                return Err(e);
            }
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
            done.databases = merge_tree(&old_ext, &home.databases())?;
        }
    }
    if old_art.is_dir() {
        done.art = merge_tree(old_art, &home.media())?;
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

/// What [`create_or_adopt`] did.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Created {
    /// A RomForge folder with a database already: used as it is.
    Existing { home: Home },
    /// A new folder, and the library from before it moved in.
    Adopted { home: Home, moved: Adopted },
    /// A new, empty folder.
    New { home: Home },
}

impl Created {
    pub fn home(&self) -> &Home {
        match self {
            Created::Existing { home } | Created::Adopted { home, .. } | Created::New { home } => home,
        }
    }
}

/// Make (or find) the RomForge folder in `parent` (the caller then records
/// it with [`Home::remember`]), moving the library in use into a new one: the
/// database `old_db` (with no connection open on it) and the art under
/// `old_art` ([`adopt`]). With a RomForge folder in use already
/// (`current`), only that one is accepted: another place is refused, as
/// the folder in use is moved with [`relocate`], never left behind.
pub fn create_or_adopt(
    parent: &Path,
    current: Option<&Home>,
    old_db: &Path,
    old_art: &Path,
) -> Result<Created, Error> {
    if let Some(h) = current {
        if root_in(parent)? != h.root {
            return Err(Error::Settings(format!(
                "already using the RomForge folder {}; to have it elsewhere, move it (home move)",
                h.root.display()
            )));
        }
        return Ok(Created::Existing { home: h.clone() });
    }
    let home = Home::create(parent)?;
    Ok(if home.db().exists() {
        Created::Existing { home }
    } else if old_db.exists() && std::path::absolute(old_db)? != home.db() {
        let moved = adopt(&home, old_db, old_art)?;
        Created::Adopted { home, moved }
    } else {
        Created::New { home }
    })
}

mod tree;
pub(crate) use tree::{merge_tree, move_tree};

#[cfg(test)]
mod tests;
