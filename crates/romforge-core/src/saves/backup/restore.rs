//! Putting a game's save folder back from one of its backup copies. A plan
//! first (nothing written); then the save there now goes to the RomForge
//! trash (`.romforge-trash/save-restore-<id>/<its full path>`), the copy is
//! written in its place (beside it as `.part`, renamed when whole), and the
//! restore is journaled (`save_restores`): undo moves the restored folder
//! to the trash too and the one taken out back. Never while the emulator
//! runs (the caller checks).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use super::{backup_dir, versions};
use crate::Error;
use crate::home::Home;
use crate::launch::existing::UserDirs;
use crate::launch::save_tree;

/// What restoring a save folder would do.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct RestorePlan {
    pub emulator: String,
    pub save: PathBuf,
    /// The copy (`<when>`), the newest unless one was asked for.
    pub version: String,
    pub from: PathBuf,
    pub to: PathBuf,
    /// The save there now goes to the trash.
    pub replaces: bool,
    /// Why it can't be done; `None`: it can.
    pub problem: Option<String>,
}

/// What putting `save` (its path in `name`'s tree) back from `version`
/// (else its newest copy) would do.
pub fn plan(
    home: &Home,
    name: &str,
    user: Option<&UserDirs>,
    save: &Path,
    version: Option<&str>,
) -> RestorePlan {
    let mut out = RestorePlan {
        emulator: name.to_owned(),
        save: save.to_path_buf(),
        ..RestorePlan::default()
    };
    let Some(tree) = save_tree(name, home, user) else {
        out.problem = Some(format!("{name} keeps no backed-up saves"));
        return out;
    };
    if save.is_absolute()
        || save
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        out.problem = Some("not a save folder of its tree".into());
        return out;
    }
    let at = backup_dir(home, &tree, name).join(save);
    let have = versions(&at);
    let Some(v) = version.map(str::to_owned).or_else(|| have.first().cloned()) else {
        out.problem = Some("it has no backup copy".into());
        return out;
    };
    if !have.contains(&v) {
        out.problem = Some(format!("it has no copy from {v}"));
        return out;
    }
    out.from = at.join(&v);
    out.to = tree.root.join(save);
    out.replaces = out.to.exists();
    out.version = v;
    out
}

/// Move a folder, across drives if need be, never over anything.
fn move_dir(from: &Path, to: &Path) -> Result<(), Error> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match crate::organize::rename_noreplace(from, to) {
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            crate::home::move_tree(from, to).map(|_| ())
        }
        r => r.map_err(Error::from),
    }
}

/// `p` in the trash of restore `id`, by its full path.
fn trashed(home: &Home, id: i64, tag: &str, p: &Path) -> PathBuf {
    let rel = p.strip_prefix("/").unwrap_or(p);
    home.trash().join(format!("save-restore-{id}{tag}")).join(rel)
}

/// Restore as [`plan`] says. Returns the restore's id.
pub fn apply(
    conn: &Connection,
    home: &Home,
    name: &str,
    user: Option<&UserDirs>,
    save: &Path,
    version: Option<&str>,
) -> Result<i64, Error> {
    let plan = plan(home, name, user, save, version);
    if let Some(why) = plan.problem {
        return Err(Error::Launch(format!("can't restore {}: {why}", save.display())));
    }
    conn.execute(
        concat!(
            "INSERT INTO save_restores (emulator, target, created_at) VALUES (?1, ?2, ",
            crate::db::now!(),
            ")"
        ),
        params![name, plan.to.to_string_lossy()],
    )?;
    let id = conn.last_insert_rowid();
    // The copy is written whole beside its place first.
    let part = plan.to.with_extension("romforge-restore.part");
    let _ = std::fs::remove_dir_all(&part);
    for e in walkdir::WalkDir::new(&plan.from).follow_links(false) {
        let e = e.map_err(std::io::Error::from)?;
        if e.file_type().is_file() {
            let target = part.join(e.path().strip_prefix(&plan.from).unwrap_or(e.path()));
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            crate::organize::copy_checked(e.path(), &target)?;
        }
    }
    if plan.replaces {
        let t = trashed(home, id, "", &plan.to);
        move_dir(&plan.to, &t)?;
        conn.execute(
            "UPDATE save_restores SET trashed = ?1 WHERE restore_id = ?2",
            params![t.to_string_lossy(), id],
        )?;
    }
    std::fs::rename(&part, &plan.to)?;
    Ok(id)
}

/// Take back the last restore of `name`'s: the restored folder goes to the
/// trash, the one it replaced comes back. Returns where.
pub fn undo(conn: &Connection, home: &Home, name: &str) -> Result<PathBuf, Error> {
    let (id, target, was): (i64, String, Option<String>) = conn
        .query_row(
            "SELECT restore_id, target, trashed FROM save_restores
              WHERE emulator = ?1 AND undone_at IS NULL ORDER BY restore_id DESC LIMIT 1",
            [name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| Error::Launch(format!("no restore of {name}'s saves to undo")))?;
    let target = PathBuf::from(target);
    if let Some(w) = &was
        && !Path::new(w).exists()
    {
        return Err(Error::Launch(
            "the save it replaced was deleted with the trash: the restore can't be undone".into(),
        ));
    }
    if target.exists() {
        move_dir(&target, &trashed(home, id, "-undone", &target))?;
    }
    if let Some(w) = was {
        move_dir(Path::new(&w), &target)?;
    }
    conn.execute(
        concat!(
            "UPDATE save_restores SET undone_at = ",
            crate::db::now!(),
            " WHERE restore_id = ?1"
        ),
        [id],
    )?;
    Ok(target)
}
