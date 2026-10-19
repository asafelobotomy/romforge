//! Migrations 36 to 51 (see `MIGRATIONS`).

pub(super) const MIGRATIONS_36_51: &[&str] = &[
    // 36: BIOS import journal (milestone 20): the files each import made,
    // so undo takes them away (an import never replaces a file).
    "CREATE TABLE firmware_batches (
        batch_id   INTEGER PRIMARY KEY,
        created_at TEXT NOT NULL,
        undone_at  TEXT
    );
    CREATE TABLE firmware_outputs (
        batch_id INTEGER NOT NULL REFERENCES firmware_batches(batch_id),
        path     TEXT NOT NULL,
        PRIMARY KEY (batch_id, path)
    );",
];
