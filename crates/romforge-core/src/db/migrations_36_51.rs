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
    // 37: "import my existing setup" journal (milestone 18, 1.3): files
    // copied into romforge's folder for an emulator, and the settings
    // file each replaced (kept beside it), so undo puts things back.
    "CREATE TABLE setup_imports (
        import_id  INTEGER PRIMARY KEY,
        emulator   TEXT NOT NULL,
        created_at TEXT NOT NULL,
        undone_at  TEXT
    );
    CREATE TABLE setup_import_files (
        import_id INTEGER NOT NULL REFERENCES setup_imports(import_id),
        path      TEXT NOT NULL,
        backup    TEXT,
        PRIMARY KEY (import_id, path)
    );",
];
