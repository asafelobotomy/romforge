//! Migrations 2 to 19 (see `MIGRATIONS`).

pub(super) const MIGRATIONS_2_19: &[&str] = &[
    // 2: file extensions per system (space-separated), from systems.rs
    "ALTER TABLE systems ADD COLUMN extensions TEXT NOT NULL DEFAULT '';",
    // 3: organize journal (milestone 2)
    "CREATE TABLE organize_batches (
        batch_id   INTEGER PRIMARY KEY,
        created_at TEXT NOT NULL,
        dest       TEXT NOT NULL,
        undone_at  TEXT
    );
    CREATE TABLE organize_moves (
        batch_id INTEGER NOT NULL REFERENCES organize_batches(batch_id),
        seq      INTEGER NOT NULL,
        src      TEXT NOT NULL,
        dst      TEXT NOT NULL,
        size     INTEGER NOT NULL,
        PRIMARY KEY (batch_id, seq)
    );",
    // 4: metadata and art (milestone 3), keyed by system and game name
    "CREATE TABLE game_meta (
        system_id INTEGER NOT NULL REFERENCES systems(system_id),
        title     TEXT NOT NULL,
        field     TEXT NOT NULL,
        value     TEXT NOT NULL,
        PRIMARY KEY (system_id, title, field)
    );
    CREATE TABLE meta_checked (
        system_id  INTEGER NOT NULL REFERENCES systems(system_id),
        title      TEXT NOT NULL,
        checked_at TEXT NOT NULL,
        PRIMARY KEY (system_id, title)
    );
    -- path NULL: looked for, not found upstream
    CREATE TABLE art (
        system_id  INTEGER NOT NULL REFERENCES systems(system_id),
        title      TEXT NOT NULL,
        kind       TEXT NOT NULL,
        path       TEXT,
        checked_at TEXT NOT NULL,
        PRIMARY KEY (system_id, title, kind)
    );",
    // 5: emulators the user added (milestone 5); args is a JSON array
    "CREATE TABLE emulators (
        emulator_id INTEGER PRIMARY KEY,
        name        TEXT NOT NULL,
        system_id   INTEGER NOT NULL REFERENCES systems(system_id),
        program     TEXT NOT NULL,
        args        TEXT NOT NULL,
        archives    INTEGER NOT NULL DEFAULT 0,
        UNIQUE (name, system_id)
    );",
    // 6: the owner's settings (Source / Library / staging folders)
    "CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );",
    // 7: size and modification time of the file on disk (the archive, for
    // members) when hashed, for incremental rescans
    "ALTER TABLE files ADD COLUMN disk_size INTEGER;
     ALTER TABLE files ADD COLUMN mtime INTEGER;
     ALTER TABLE scan_roots ADD COLUMN all_files INTEGER NOT NULL DEFAULT 0;",
    // 8: Wikidata matches (status: matched, ambiguous, none, by_you) and the
    // members of the series matched games belong to
    "CREATE TABLE wikidata_match (
        system_id  INTEGER NOT NULL REFERENCES systems(system_id),
        title      TEXT NOT NULL,
        qid        TEXT,
        status     TEXT NOT NULL,
        candidates TEXT NOT NULL DEFAULT '[]',
        checked_at TEXT NOT NULL,
        PRIMARY KEY (system_id, title)
    );
    CREATE TABLE series_members (
        series_qid TEXT NOT NULL,
        qid        TEXT NOT NULL,
        title      TEXT NOT NULL,
        ordinal    TEXT,
        year       INTEGER,
        PRIMARY KEY (series_qid, qid)
    );",
    // 9: titles the owner gave files no DAT knows (Search), by content so
    // they survive moves and rescans
    "CREATE TABLE manual_ids (
        sha1       TEXT NOT NULL,
        size       INTEGER NOT NULL,
        system_id  INTEGER NOT NULL REFERENCES systems(system_id),
        title      TEXT NOT NULL,
        created_at TEXT NOT NULL,
        PRIMARY KEY (sha1, size)
    );
    -- files.status gains 'manual': SQLite can't change a CHECK, so the table
    -- is rebuilt (matches are kept aside, as dropping files cascades to them)
    CREATE TEMP TABLE matches_keep AS SELECT * FROM matches;
    CREATE TABLE files_new (
        file_id        INTEGER PRIMARY KEY,
        root_id        INTEGER NOT NULL REFERENCES scan_roots(root_id) ON DELETE CASCADE,
        path           TEXT NOT NULL,
        member         TEXT NOT NULL DEFAULT '',
        size           INTEGER NOT NULL,
        crc32          TEXT NOT NULL,
        md5            TEXT NOT NULL,
        sha1           TEXT NOT NULL,
        header_kind    TEXT,
        stripped_size  INTEGER,
        stripped_crc32 TEXT,
        stripped_md5   TEXT,
        stripped_sha1  TEXT,
        status         TEXT NOT NULL DEFAULT 'unknown'
            CHECK (status IN ('matched', 'candidate', 'unknown', 'wrong_system', 'duplicate', 'manual')),
        scanned_at     TEXT NOT NULL,
        disk_size      INTEGER,
        mtime          INTEGER,
        UNIQUE (path, member)
    );
    INSERT INTO files_new SELECT file_id, root_id, path, member, size, crc32, md5, sha1, header_kind,
        stripped_size, stripped_crc32, stripped_md5, stripped_sha1, status, scanned_at, disk_size, mtime
        FROM files;
    DROP TABLE files;
    ALTER TABLE files_new RENAME TO files;
    CREATE INDEX files_root ON files(root_id);
    INSERT INTO matches SELECT * FROM matches_keep;
    DROP TABLE matches_keep;",
    // 10: where each image came from, and which keyed art sources were
    // asked about a game
    "ALTER TABLE art ADD COLUMN source TEXT;
    CREATE TABLE art_lookup (
        system_id  INTEGER NOT NULL REFERENCES systems(system_id),
        title      TEXT NOT NULL,
        provider   TEXT NOT NULL,
        checked_at TEXT NOT NULL,
        PRIMARY KEY (system_id, title, provider)
    );",
    // 11: cleanup & rename journal (milestone 10)
    "CREATE TABLE cleanup_batches (
        batch_id       INTEGER PRIMARY KEY,
        created_at     TEXT NOT NULL,
        keep_originals INTEGER NOT NULL,
        undone_at      TEXT
    );
    CREATE TABLE cleanup_outputs (
        batch_id INTEGER NOT NULL REFERENCES cleanup_batches(batch_id),
        path     TEXT NOT NULL,
        kind     TEXT NOT NULL CHECK (kind IN ('file', 'dir')),
        PRIMARY KEY (batch_id, path)
    );
    CREATE TABLE cleanup_trash (
        batch_id INTEGER NOT NULL REFERENCES cleanup_batches(batch_id),
        original TEXT NOT NULL,
        trashed  TEXT NOT NULL,
        PRIMARY KEY (batch_id, trashed)
    );",
    // 12: Wikidata candidates per platform item, and the (normalized) titles
    // already searched there, so a run cut short by a rate limit resumes
    "CREATE TABLE wikidata_candidates (
        platform TEXT NOT NULL,
        qid      TEXT NOT NULL,
        label    TEXT NOT NULL,
        names    TEXT NOT NULL,
        years    TEXT NOT NULL,
        PRIMARY KEY (platform, qid)
    );
    CREATE TABLE wikidata_searched (
        platform    TEXT NOT NULL,
        name        TEXT NOT NULL,
        searched_at TEXT NOT NULL,
        PRIMARY KEY (platform, name)
    );",
    // 13: where a metadata field came from (NULL: libretro or Wikidata as
    // before; 'openvgdb', 'launchbox' for the databases the user downloads)
    "ALTER TABLE game_meta ADD COLUMN source TEXT;",
    // 14: N64 byte orders and .smd interleave are normalized now: files
    // that may be either are hashed again on their next scan (every row of
    // an archive, since the stamp is the archive's).
    "UPDATE files SET mtime = NULL WHERE path IN (
       SELECT path FROM files
       WHERE lower(CASE WHEN member = '' THEN path ELSE member END) GLOB '*.[vn]64'
          OR lower(CASE WHEN member = '' THEN path ELSE member END) GLOB '*.smd');",
    // 15: more of each DAT game (parent, region, Redump category), and
    // product codes by system and title, from the DATs and libretro's
    // serial lists; kept by title so they outlive a DAT's re-import.
    // DATs are read again on their next download (their stored hash is
    // cleared) so the new fields fill in.
    "ALTER TABLE games ADD COLUMN cloneof TEXT;
     ALTER TABLE games ADD COLUMN region TEXT;
     ALTER TABLE games ADD COLUMN category TEXT;
     CREATE TABLE serials (
         system_id INTEGER NOT NULL REFERENCES systems(system_id),
         title     TEXT NOT NULL,
         serial    TEXT NOT NULL,
         -- letters and digits only, upper case (dat::serial_key)
         key       TEXT NOT NULL,
         -- 'dat' or 'libretro'
         source    TEXT NOT NULL,
         PRIMARY KEY (system_id, title, serial)
     );
     CREATE INDEX serials_key ON serials(key);
     UPDATE dats SET source_sha256 = '';",
    // 16: what each file's own header or boot files say it is (ident.rs).
    // Files that aren't matched yet are read again on their next scan; a
    // matched file needs no such evidence (`--rehash` fills it in too).
    "ALTER TABLE files ADD COLUMN id_kind TEXT;
     ALTER TABLE files ADD COLUMN id_serial TEXT;
     ALTER TABLE files ADD COLUMN id_title TEXT;
     ALTER TABLE files ADD COLUMN id_region TEXT;
     UPDATE files SET mtime = NULL WHERE path IN (
       SELECT path FROM files WHERE status NOT IN ('matched', 'duplicate'));",
    // 17: a file named by the owner as a translation or hack of a DAT
    // game (`base`), with its tag ("English v2.0 Addendum").
    "ALTER TABLE manual_ids ADD COLUMN base TEXT;
     ALTER TABLE manual_ids ADD COLUMN relation TEXT;
     ALTER TABLE manual_ids ADD COLUMN tag TEXT;",
    // 18: a cue sheet's FILE names (JSON), to tell a rewritten sheet that
    // fits its game's tracks (status 'sheet'). SQLite can't change a CHECK,
    // so the table is rebuilt as in 9 (matches kept aside). Cue sheets not
    // matched are read again on their next scan.
    "CREATE TEMP TABLE matches_keep AS SELECT * FROM matches;
     CREATE TABLE files_new (
        file_id        INTEGER PRIMARY KEY,
        root_id        INTEGER NOT NULL REFERENCES scan_roots(root_id) ON DELETE CASCADE,
        path           TEXT NOT NULL,
        member         TEXT NOT NULL DEFAULT '',
        size           INTEGER NOT NULL,
        crc32          TEXT NOT NULL,
        md5            TEXT NOT NULL,
        sha1           TEXT NOT NULL,
        header_kind    TEXT,
        stripped_size  INTEGER,
        stripped_crc32 TEXT,
        stripped_md5   TEXT,
        stripped_sha1  TEXT,
        status         TEXT NOT NULL DEFAULT 'unknown'
            CHECK (status IN ('matched', 'candidate', 'unknown', 'wrong_system', 'duplicate', 'manual', 'sheet')),
        scanned_at     TEXT NOT NULL,
        disk_size      INTEGER,
        mtime          INTEGER,
        id_kind        TEXT,
        id_serial      TEXT,
        id_title       TEXT,
        id_region      TEXT,
        sheet_files    TEXT,
        UNIQUE (path, member)
     );
     INSERT INTO files_new SELECT file_id, root_id, path, member, size, crc32, md5, sha1, header_kind,
        stripped_size, stripped_crc32, stripped_md5, stripped_sha1, status, scanned_at, disk_size, mtime,
        id_kind, id_serial, id_title, id_region, NULL
        FROM files;
     DROP TABLE files;
     ALTER TABLE files_new RENAME TO files;
     CREATE INDEX files_root ON files(root_id);
     INSERT INTO matches SELECT * FROM matches_keep;
     DROP TABLE matches_keep;
     UPDATE files SET mtime = NULL WHERE path IN (
       SELECT path FROM files WHERE status <> 'matched'
         AND lower(CASE WHEN member = '' THEN path ELSE member END) GLOB '*.cue');",
    // 19: the plain image's size a compressed disc image (RVZ, WIA, GCZ,
    // CSO, ZSO) holds; container files are read again on their next scan.
    "ALTER TABLE files ADD COLUMN id_size INTEGER;
     UPDATE files SET mtime = NULL WHERE member = '' AND (
       lower(path) GLOB '*.rvz' OR lower(path) GLOB '*.wia' OR lower(path) GLOB '*.wbfs'
       OR lower(path) GLOB '*.ciso' OR lower(path) GLOB '*.gcz' OR lower(path) GLOB '*.cso'
       OR lower(path) GLOB '*.zso' OR lower(path) GLOB '*.pbp');",
];
