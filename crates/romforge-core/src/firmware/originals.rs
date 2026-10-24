//! A BIOS import's originals when Settings says not to leave BIOS files
//! where they are: each dropped or found file (a plain file, never a member
//! of an archive) whose bytes are now in `Bios/` moves to the RomForge
//! trash (`.romforge-trash/bios-<batch>/<its full path>`), journaled so
//! undo puts it back and Empty trash deletes it.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use super::digest;
use super::import::{ImportPlan, Match, TargetState};
use crate::Error;
use crate::home::Home;

/// The plan's originals that an import takes (accepted, or ticked), with
/// the places that hold or will hold them.
pub(super) fn taken(plan: &ImportPlan, also: &[String]) -> Vec<(PathBuf, String, Vec<PathBuf>)> {
    plan.files
        .iter()
        .filter(|f| f.source.members.is_empty())
        .filter(|f| f.matched == Match::Accepted || (f.matched == Match::NameOnly && also.contains(&f.shown)))
        .filter_map(|f| {
            let places: Vec<PathBuf> = f
                .targets
                .iter()
                .filter(|t| matches!(t.state, TargetState::New | TargetState::AlreadyThere))
                .map(|t| t.place.clone())
                .collect();
            (!places.is_empty()).then(|| (f.source.file.clone(), f.sha1.clone(), places))
        })
        .collect()
}

/// Move each of `originals` to the trash once a place holds its bytes.
/// Returns how many moved.
pub(super) fn trash(
    conn: &Connection,
    home: &Home,
    batch: i64,
    originals: &[(PathBuf, String, Vec<PathBuf>)],
) -> Result<usize, Error> {
    let mut n = 0;
    for (original, sha1, places) in originals {
        if !places.iter().any(|p| digest(p).is_some_and(|d| d.sha1 == *sha1)) {
            continue; // not in Bios/ after all: it stays
        }
        let rel = original.strip_prefix("/").unwrap_or(original);
        let to = home.trash().join(format!("bios-{batch}")).join(rel);
        crate::organize::move_file(original, &to)?;
        conn.execute(
            "INSERT INTO firmware_trash (batch_id, original, trashed) VALUES (?1, ?2, ?3)",
            params![batch, original.to_string_lossy(), to.to_string_lossy()],
        )?;
        n += 1;
    }
    Ok(n)
}

/// Put batch `batch`'s trashed originals back. Returns how many.
pub(super) fn restore(conn: &Connection, batch: i64) -> Result<usize, Error> {
    let rows: Vec<(String, String)> = conn
        .prepare("SELECT original, trashed FROM firmware_trash WHERE batch_id = ?1")?
        .query_map([batch], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut n = 0;
    for (original, trashed) in rows {
        crate::organize::move_file(Path::new(&trashed), Path::new(&original))?;
        conn.execute(
            "DELETE FROM firmware_trash WHERE batch_id = ?1 AND trashed = ?2",
            params![batch, trashed],
        )?;
        n += 1;
    }
    Ok(n)
}
