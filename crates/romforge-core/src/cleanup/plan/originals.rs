//! Which originals of a plan go to the trash.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::Error;
use crate::view::RowStatus;

use super::super::Item;
use super::super::items::archive_names;

/// Originals: trashed only when everything in them is taken by the batch
/// (an archive's own member list decides, not just what was scanned) and
/// no game outside the batch needs any of their files (a BIOS or parent
/// zip of an arcade set included). `covered`: rows with nothing of their
/// own to take, in the batch when all their files are. `files_at`: the
/// file ids at a path.
pub(super) fn mark(
    items: &mut [Item],
    covered: &[(String, Vec<PathBuf>)],
    rows_of: &HashMap<i64, Vec<(String, String, RowStatus)>>,
    in_source: &dyn Fn(&Path) -> bool,
    mut files_at: impl FnMut(&Path) -> Result<Vec<i64>, Error>,
) -> Result<(), Error> {
    let mut used: HashMap<PathBuf, BTreeSet<String>> = HashMap::new();
    for it in items.iter().filter(|it| it.folder.is_none()) {
        for p in it.parts.iter().chain(&it.copies).chain(&it.sheet) {
            used.entry(p.path.clone()).or_default().insert(p.member.clone());
        }
    }
    let mut batch: BTreeSet<String> = items.iter().map(|it| it.row_id.clone()).collect();
    for (id, paths) in covered {
        if paths.iter().all(|p| !in_source(p) || used.contains_key(p)) {
            batch.insert(id.clone());
        }
    }
    for it in items.iter_mut().filter(|it| it.folder.is_none()) {
        let paths: BTreeSet<PathBuf> = it
            .parts
            .iter()
            .chain(&it.copies)
            .chain(&it.sheet)
            .map(|p| p.path.clone())
            .collect();
        for p in paths {
            if !in_source(&p) {
                continue;
            }
            let taken = &used[&p];
            let all = if taken.contains("") {
                true // a plain file
            } else {
                archive_names(&p).is_ok_and(|names| names.iter().all(|n| taken.contains(n)))
            };
            // Any of its files in a game outside the batch (a mere
            // duplicate copy doesn't count) keeps it.
            let needed_elsewhere = files_at(&p)?
                .iter()
                .flat_map(|f| rows_of.get(f).into_iter().flatten())
                .any(|(id, _, status)| !batch.contains(id) && *status != RowStatus::Duplicate);
            if all && !needed_elsewhere {
                it.trash.push(p);
            } else {
                it.kept.push(p);
            }
        }
    }
    // An archive shared by two games in the batch is trashed once, by the
    // last of them (after both are written).
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    for it in items.iter_mut().rev() {
        it.trash.retain(|p| seen.insert(p.clone()));
    }
    Ok(())
}
