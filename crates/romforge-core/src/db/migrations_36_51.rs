//! Migrations from 36 on (36 to 42 so far; the file is named for the range
//! it will hold, up to 51; see `MIGRATIONS`).

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
    // 42: what the user keeps per game (stage 2, step 6): a favourite, the
    // emulator picked for it. Apart from Enrich's `game_meta`, which a
    // refresh rewrites.
    "CREATE TABLE user_games (
        system TEXT NOT NULL,
        title  TEXT NOT NULL,
        key    TEXT NOT NULL,
        value  TEXT NOT NULL,
        PRIMARY KEY (system, title, key)
    );",
    // 43: milestone 20's gaps. A name-only BIOS file you ticked is
    // journaled as yours (`picked`), so the page says "placed by you"; the
    // originals a BIOS import moved (Settings: not leaving them where they
    // are) are kept in the trash for undo; and moving an emulator's game
    // data into RomForge is journaled, its originals in the trash.
    "ALTER TABLE firmware_outputs ADD COLUMN picked INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE firmware_batches ADD COLUMN emptied_at TEXT;
    CREATE TABLE firmware_trash (
        batch_id INTEGER NOT NULL REFERENCES firmware_batches(batch_id),
        original TEXT NOT NULL,
        trashed  TEXT NOT NULL,
        PRIMARY KEY (batch_id, trashed)
    );
    CREATE TABLE data_moves (
        move_id    INTEGER PRIMARY KEY,
        emulator   TEXT NOT NULL,
        created_at TEXT NOT NULL,
        undone_at  TEXT,
        emptied_at TEXT
    );
    CREATE TABLE data_move_items (
        move_id  INTEGER NOT NULL REFERENCES data_moves(move_id),
        original TEXT NOT NULL,
        copy     TEXT NOT NULL,
        trashed  TEXT,
        PRIMARY KEY (move_id, original)
    );",
    // 44: milestone 21's All settings: each key changed by hand in
    // romforge's copy of an emulator's config, with the value it had
    // (NULL: it wasn't there), so undo puts it back. One save is a batch.
    "CREATE TABLE config_edits (
        edit_id    INTEGER PRIMARY KEY,
        batch      INTEGER NOT NULL,
        emulator   TEXT NOT NULL,
        file       TEXT NOT NULL,
        key        TEXT NOT NULL,
        old_value  TEXT,
        new_value  TEXT,
        created_at TEXT NOT NULL,
        undone_at  TEXT
    );
    CREATE INDEX config_edits_by_emulator ON config_edits (emulator, batch);",
    // 45: milestone 20, step 6: RetroBIOS's system files by size and hash,
    // from the list downloaded (`firmware::index`), so a scanned file that
    // is one shows as a BIOS file (house rule 8: size + SHA1 or MD5).
    "CREATE TABLE firmware_hashes (
        size      INTEGER NOT NULL,
        sha1      TEXT,
        md5       TEXT,
        system_id INTEGER NOT NULL,
        name      TEXT NOT NULL
    );
    CREATE INDEX firmware_hashes_sha1 ON firmware_hashes (sha1, size);
    CREATE INDEX firmware_hashes_md5 ON firmware_hashes (md5, size);",
    // 46: milestone 27, stage 1: an emulator's saves gathered into Saves/,
    // each file journaled so undo puts it back (`saves::gather`).
    "CREATE TABLE save_gathers (
        gather_id  INTEGER PRIMARY KEY,
        emulator   TEXT NOT NULL,
        created_at TEXT NOT NULL,
        undone_at  TEXT
    );
    CREATE TABLE save_gather_files (
        gather_id INTEGER NOT NULL REFERENCES save_gathers(gather_id),
        original  TEXT NOT NULL,
        moved_to  TEXT NOT NULL,
        PRIMARY KEY (gather_id, original)
    );",
];
