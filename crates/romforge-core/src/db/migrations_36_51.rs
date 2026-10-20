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
    // 38: play time (milestone 18, 1.6): each time a game was played, from
    // its emulator's process lifetime, by system key and game title (what
    // the library row and the launcher both name it by).
    "CREATE TABLE play_sessions (
        session_id INTEGER PRIMARY KEY,
        system     TEXT NOT NULL,
        game       TEXT NOT NULL,
        emulator   TEXT NOT NULL,
        started_at INTEGER NOT NULL,
        seconds    INTEGER NOT NULL
    );
    CREATE INDEX play_sessions_game ON play_sessions (system, game);",
    // 39: manuals (milestone 26): a game's PDF, where it came from, its
    // Vimm's Lair id from Hasheous (asked once: `checked_at`) and the page
    // last read, by system key and title.
    "CREATE TABLE manuals (
        system     TEXT NOT NULL,
        game       TEXT NOT NULL,
        path       TEXT,
        source     TEXT,
        vimm_id    TEXT,
        checked_at TEXT,
        last_page  INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (system, game)
    );",
    // 40: a manual's marks (milestone 26): highlighted text and notes on an
    // area of a page, placed as fractions of the page (so they hold at any
    // zoom), by system key and title like the manual itself.
    "CREATE TABLE manual_marks (
        mark_id    INTEGER PRIMARY KEY,
        system     TEXT NOT NULL,
        game       TEXT NOT NULL,
        page       INTEGER NOT NULL,
        kind       TEXT NOT NULL CHECK (kind IN ('highlight', 'area')),
        text       TEXT NOT NULL DEFAULT '',
        rects      TEXT NOT NULL,
        color      TEXT NOT NULL,
        note       TEXT NOT NULL DEFAULT '',
        created_at TEXT NOT NULL
    );
    CREATE INDEX manual_marks_game ON manual_marks (system, game, page);",
    // 41: OCR of a manual's scanned pages (milestone 26): the words
    // recognised on a page in a language, each with its box as fractions
    // of the page, so a scan can be searched and highlighted.
    "CREATE TABLE manual_ocr (
        system     TEXT NOT NULL,
        game       TEXT NOT NULL,
        page       INTEGER NOT NULL,
        lang       TEXT NOT NULL,
        words      TEXT NOT NULL,
        created_at TEXT NOT NULL,
        PRIMARY KEY (system, game, page, lang)
    );",
];
