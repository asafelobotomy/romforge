//! SQLite schema. Hash columns are indexed but never UNIQUE: one hash can
//! belong to many games, in one DAT or across DATs.

use rusqlite::{Connection, params};

use crate::Error;
use crate::systems::SYSTEMS;

/// Schema changes after version 1, in order. Entry `i` upgrades to version `i + 2`.
const MIGRATIONS: &[&str] = &[
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
];

const SCHEMA_VERSION: i64 = 1 + MIGRATIONS.len() as i64;

const SCHEMA: &str = r#"
CREATE TABLE systems (
    system_id      INTEGER PRIMARY KEY,
    key            TEXT NOT NULL UNIQUE,
    name           TEXT NOT NULL,
    manufacturer   TEXT NOT NULL,
    generation     INTEGER NOT NULL,
    release_year   INTEGER NOT NULL,
    preferred_hash TEXT NOT NULL,
    is_disc_based  INTEGER NOT NULL,
    is_handheld    INTEGER NOT NULL
);

-- One row per imported DAT file: where it came from and when.
CREATE TABLE dats (
    dat_id        INTEGER PRIMARY KEY,
    system_id     INTEGER REFERENCES systems(system_id),
    name          TEXT NOT NULL,
    description   TEXT,
    version       TEXT,
    author        TEXT,
    homepage      TEXT,
    url           TEXT,
    header_hint   TEXT,
    source_path   TEXT NOT NULL,
    source_sha256 TEXT NOT NULL,
    imported_at   TEXT NOT NULL
);

CREATE TABLE games (
    game_id     INTEGER PRIMARY KEY,
    dat_id      INTEGER NOT NULL REFERENCES dats(dat_id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    description TEXT
);
CREATE INDEX games_dat ON games(dat_id);

CREATE TABLE roms (
    rom_id  INTEGER PRIMARY KEY,
    game_id INTEGER NOT NULL REFERENCES games(game_id) ON DELETE CASCADE,
    name    TEXT NOT NULL,
    size    INTEGER,
    crc32   TEXT,
    md5     TEXT,
    sha1    TEXT,
    status  TEXT
);
CREATE INDEX roms_game ON roms(game_id);
CREATE INDEX roms_sha1 ON roms(sha1);
CREATE INDEX roms_md5 ON roms(md5);
CREATE INDEX roms_crc32 ON roms(crc32);

-- A scanned folder and the identification scope chosen for it.
-- Both NULL means "all DATs"; there is no default system.
CREATE TABLE scan_roots (
    root_id   INTEGER PRIMARY KEY,
    path      TEXT NOT NULL UNIQUE,
    system_id INTEGER REFERENCES systems(system_id),
    dat_id    INTEGER REFERENCES dats(dat_id) ON DELETE SET NULL,
    scanned_at TEXT NOT NULL
);

-- One row per hashed file or archive member. `member` is '' for plain files.
CREATE TABLE files (
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
        CHECK (status IN ('matched', 'candidate', 'unknown', 'wrong_system', 'duplicate')),
    scanned_at     TEXT NOT NULL,
    UNIQUE (path, member)
);
CREATE INDEX files_root ON files(root_id);

-- Every hit for a file; a file may hit many ROMs.
CREATE TABLE matches (
    file_id INTEGER NOT NULL REFERENCES files(file_id) ON DELETE CASCADE,
    rom_id  INTEGER NOT NULL REFERENCES roms(rom_id) ON DELETE CASCADE,
    -- 'sha1', 'md5' or 'crc32'
    via     TEXT NOT NULL,
    -- 1 when the stripped (headerless) hashes matched
    stripped INTEGER NOT NULL,
    -- 1 when the ROM is outside the file's scan scope
    out_of_scope INTEGER NOT NULL,
    PRIMARY KEY (file_id, rom_id)
);
CREATE INDEX matches_rom ON matches(rom_id);
"#;

pub fn open(path: &std::path::Path) -> Result<Connection, Error> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    init(Connection::open(path)?)
}

pub fn open_in_memory() -> Result<Connection, Error> {
    init(Connection::open_in_memory()?)
}

fn init(mut conn: Connection) -> Result<Connection, Error> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(Error::Schema(version));
    }
    let tx = conn.transaction()?;
    if version == 0 {
        tx.execute_batch(SCHEMA)?;
    }
    for (i, sql) in MIGRATIONS.iter().enumerate() {
        if version < i as i64 + 2 {
            tx.execute_batch(sql)?;
        }
    }
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    seed_systems(&tx)?;
    tx.commit()?;
    Ok(conn)
}

fn seed_systems(conn: &Connection) -> Result<(), Error> {
    let mut stmt = conn.prepare(
        "INSERT INTO systems (system_id, key, name, manufacturer, generation, release_year,
                              preferred_hash, is_disc_based, is_handheld, extensions)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(system_id) DO UPDATE SET key = excluded.key, name = excluded.name,
           manufacturer = excluded.manufacturer, generation = excluded.generation,
           release_year = excluded.release_year, preferred_hash = excluded.preferred_hash,
           is_disc_based = excluded.is_disc_based, is_handheld = excluded.is_handheld,
           extensions = excluded.extensions",
    )?;
    for s in SYSTEMS {
        stmt.execute(params![
            s.id,
            s.key,
            s.name,
            s.manufacturer,
            s.generation,
            s.release_year,
            s.preferred_hash,
            s.is_disc_based,
            s.is_handheld,
            s.extensions.join(" ")
        ])?;
    }
    Ok(())
}
