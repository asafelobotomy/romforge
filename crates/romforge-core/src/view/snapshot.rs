//! Fingerprint of what the list shows, and the core-only parts of a row
//! kept with [`LibraryView`] snapshots.

use rusqlite::Connection;

use super::{GameRow, LibraryView, Relation};
use crate::Error;

/// A fingerprint of everything [`listing`](super::listing) shows: each table it reads
/// (only the columns it uses), counted and summed through `fp()`, so the
/// list is built again only when this changes (a rescan that finds nothing
/// new, Enrich's bookkeeping, or an unrelated setting leave it alone). A
/// few milliseconds for tens of thousands of files, against a second or
/// more to build the list.
pub fn fingerprint(conn: &Connection) -> Result<u64, Error> {
    const PARTS: &[&str] = &[
        "SELECT count(*), total(fp(file_id, path, member, size, sha1, status)) FROM files",
        "SELECT count(*), total(fp(file_id, rom_id, via, stripped, out_of_scope, owns)) FROM matches",
        "SELECT count(*), total(fp(sha1, size, system_id, title, base, relation, tag)) FROM manual_ids",
        "SELECT count(*), total(fp(path, name, size, relation, system_id, title)) FROM patch_files",
        "SELECT count(*), total(fp(folder_id, path, system_id, title, files, size)) FROM folder_games",
        "SELECT count(*), total(fp(system_id, title, kind, path)) FROM art",
        "SELECT count(*), total(fp(system_id, title, field, value)) FROM game_meta",
        "SELECT count(*), total(fp(system_id, title, grp, priority, list)) FROM clone_groups",
        "SELECT count(*), total(fp(dat_id, system_id, name)) FROM dats",
        "SELECT count(*), total(fp(key, value)) FROM settings WHERE key = 'regions'",
        // Folder games are found under the scanned folders (a nested one
        // changes which). `games` needs no line: a DAT imported again gets
        // a new `dat_id`, so `dats` changes with its games.
        "SELECT count(*), total(fp(path)) FROM scan_roots",
    ];
    let mut text = String::new();
    for q in PARTS {
        let (n, sum): (i64, f64) = conn.query_row(q, [], |r| Ok((r.get(0)?, r.get(1)?)))?;
        text += &format!("{n}:{sum};");
    }
    // FNV-1a of the parts.
    Ok(text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    }))
}

/// What a row keeps for the core alone (not sent to the app): its system
/// id, files and patch's game, for [`LibraryView`] snapshots.
pub(crate) type RowInner = (Option<i64>, Vec<i64>, Option<(String, Relation)>);

impl LibraryView {
    pub(crate) fn inner(&self) -> Vec<RowInner> {
        self.rows
            .iter()
            .map(|r| (r.system_id, r.file_ids.clone(), r.patch_of.clone()))
            .collect()
    }

    /// Put back what [`LibraryView::inner`] took; `false` when it doesn't fit.
    pub(crate) fn restore(&mut self, inner: Vec<RowInner>) -> bool {
        if inner.len() != self.rows.len() {
            return false;
        }
        for (r, (s, f, p)) in self.rows.iter_mut().zip(inner) {
            r.system_id = s;
            r.file_ids = f;
            r.patch_of = p;
        }
        true
    }
}

impl GameRow {
    /// The row's files in `files`.
    pub(crate) fn file_ids(&self) -> &[i64] {
        &self.file_ids
    }
}
