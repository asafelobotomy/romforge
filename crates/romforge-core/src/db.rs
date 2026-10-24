//! SQLite schema. Hash columns are indexed but never UNIQUE: one hash can
//! belong to many games, in one DAT or across DATs.

use rusqlite::{Connection, params};

use crate::Error;
use crate::systems::SYSTEMS;

/// The current time in UTC as SQL (`2026-10-18T22:14:54Z`), for
/// `concat!` into a query: every timestamp romforge stores looks the same.
macro_rules! now {
    () => {
        "strftime('%Y-%m-%dT%H:%M:%SZ', 'now')"
    };
}
pub(crate) use now;

/// The current time in UTC (`2026-10-18T22:14:54Z`) without a database
/// at hand, formatted by SQLite as every stored timestamp is.
pub(crate) fn now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    Connection::open_in_memory()
        .and_then(|c| {
            c.query_row(
                "SELECT strftime('%Y-%m-%dT%H:%M:%SZ', ?1, 'unixepoch')",
                [secs],
                |r| r.get(0),
            )
        })
        .unwrap_or_default()
}

mod migrations_20_35;
mod migrations_2_19;
mod migrations_36_51;

/// Schema changes after version 1, in order. Entry `i` upgrades to version `i + 2`.
const MIGRATIONS: &[&str] = &concat::<
    {
        migrations_2_19::MIGRATIONS_2_19.len()
            + migrations_20_35::MIGRATIONS_20_35.len()
            + migrations_36_51::MIGRATIONS_36_51.len()
    },
>(&[
    migrations_2_19::MIGRATIONS_2_19,
    migrations_20_35::MIGRATIONS_20_35,
    migrations_36_51::MIGRATIONS_36_51,
]);

/// The migration files' lists, one after the other.
const fn concat<const N: usize>(parts: &[&[&'static str]]) -> [&'static str; N] {
    let mut out = [""; N];
    let (mut p, mut n) = (0, 0);
    while p < parts.len() {
        let mut i = 0;
        while i < parts[p].len() {
            out[n] = parts[p][i];
            (i, n) = (i + 1, n + 1);
        }
        p += 1;
    }
    assert!(n == N);
    out
}

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
    -- 'sha1', 'md5' or 'crc32'; 'sheet' for a rewritten cue sheet (migration 18);
    -- 'confirmed' for a CRC32-only arcade ROM whose SHA1 fits MAME's same ROM,
    -- 'disk' for an arcade disk by its CHD's SHA1 (see report::strong!)
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

/// `fp(a, b, ...)`: a 31-bit hash of its arguments (FNV-1a over their
/// types and values), for [`crate::view::fingerprint`] to `sum()` over a
/// table without overflowing.
fn fingerprint_function(conn: &Connection) -> Result<(), Error> {
    use rusqlite::functions::FunctionFlags;
    use rusqlite::types::ValueRef;
    conn.create_scalar_function(
        "fp",
        -1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            let mut eat = |bytes: &[u8]| {
                for b in bytes {
                    h ^= u64::from(*b);
                    h = h.wrapping_mul(0x0100_0000_01b3);
                }
            };
            for i in 0..ctx.len() {
                match ctx.get_raw(i) {
                    ValueRef::Null => eat(&[0]),
                    ValueRef::Integer(v) => {
                        eat(&[1]);
                        eat(&v.to_le_bytes());
                    }
                    ValueRef::Real(v) => {
                        eat(&[2]);
                        eat(&v.to_le_bytes());
                    }
                    ValueRef::Text(t) => {
                        eat(&[3]);
                        eat(t);
                    }
                    ValueRef::Blob(b) => {
                        eat(&[4]);
                        eat(b);
                    }
                }
                eat(&[0x1f]);
            }
            Ok((h >> 33) as i64)
        },
    )?;
    Ok(())
}

pub fn open_in_memory() -> Result<Connection, Error> {
    init(Connection::open_in_memory()?)
}

fn init(mut conn: Connection) -> Result<Connection, Error> {
    fingerprint_function(&conn)?;
    // Another connection (the app keeps one for reads) may be writing:
    // wait for it rather than fail with "database is locked".
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    // Every write takes SQLite's write lock when it begins: a transaction
    // that read first and then found another connection had written since
    // would fail at once ("database is locked"), with no wait at all. Quick
    // edits on their own connection beside a long job rely on this.
    conn.set_transaction_behavior(rusqlite::TransactionBehavior::Immediate);
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(Error::Schema(version));
    }
    // Up to date already: no write, so opening never waits for another
    // connection's long transaction (a scan holds one for its whole run).
    if version == SCHEMA_VERSION && systems_current(&conn)? {
        return Ok(conn);
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

/// Whether the `systems` table already holds exactly [`SYSTEMS`].
fn systems_current(conn: &Connection) -> Result<bool, Error> {
    type Row = (i64, String, String, String, i64, i64, String, bool, bool, String);
    let stored: Vec<Row> = conn
        .prepare(
            "SELECT system_id, key, name, manufacturer, generation, release_year,
                    preferred_hash, is_disc_based, is_handheld, extensions
             FROM systems ORDER BY system_id",
        )?
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    let mut code: Vec<Row> = SYSTEMS
        .iter()
        .map(|s| {
            (
                s.id,
                s.key.to_owned(),
                s.name.to_owned(),
                s.manufacturer.to_owned(),
                s.generation,
                s.release_year,
                s.preferred_hash.to_owned(),
                s.is_disc_based,
                s.is_handheld,
                s.extensions.join(" "),
            )
        })
        .collect();
    code.sort_by_key(|r| r.0);
    Ok(stored == code)
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
