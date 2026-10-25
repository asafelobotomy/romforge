//! Stored paths that follow the RomForge folder when it moves.

use std::path::Path;

use rusqlite::params;

use crate::Error;

/// Where the database last saw its folder (a `settings` row).
const ROOT_KEY: &str = "home_root";

/// Every stored path, by table and column: rewritten when the folder moves.
pub(super) const PATH_COLUMNS: &[(&str, &str)] = &[
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
    ("manuals", "path"),
    ("firmware_outputs", "path"),
    ("setup_import_files", "path"),
    ("setup_import_files", "backup"),
    ("firmware_trash", "original"),
    ("firmware_trash", "trashed"),
    ("data_move_items", "original"),
    ("data_move_items", "copy"),
    ("data_move_items", "trashed"),
    ("save_gather_files", "original"),
    ("save_gather_files", "moved_to"),
    ("save_restores", "target"),
    ("save_restores", "trashed"),
    // An emulator set up by hand may be a program in the folder.
    ("emulators", "program"),
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
    // Deferred: opening a connection must not wait on another one's write
    // (a scan's) when there is nothing to fix.
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;
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
