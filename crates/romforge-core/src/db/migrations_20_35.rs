//! Migrations 20 to 35 (see `MIGRATIONS`).

pub(super) const MIGRATIONS_20_35: &[&str] = &[
    // 20: GDI sheets are read for their track names too (a user's GDI fits
    // a Dreamcast game like a rewritten cue); those not matched are read
    // again.
    // An NKit image's original-image CRC32 (id_crc); GameCube / Wii files
    // not matched are read again too.
    "ALTER TABLE files ADD COLUMN id_crc INTEGER;
     UPDATE files SET mtime = NULL WHERE path IN (
       SELECT path FROM files WHERE status <> 'matched'
         AND (lower(CASE WHEN member = '' THEN path ELSE member END) GLOB '*.gdi'
              OR id_kind IN ('gc', 'wii')));",
    // 21: Retool's clone lists (BSD-3-Clause): titles that are one game
    // under different names (regions, retitles), for 1G1R. `title` is the
    // title without tags, lower case; a lower `priority` is preferred.
    "CREATE TABLE clone_groups (
         system_id INTEGER NOT NULL REFERENCES systems(system_id),
         title     TEXT NOT NULL,
         grp       TEXT NOT NULL,
         priority  INTEGER NOT NULL DEFAULT 1,
         PRIMARY KEY (system_id, title)
     );",
    // 22: games by name (details, cleanup, suggestions look games up by
    // title: a full scan of a million TOSEC and Redump games per click).
    "CREATE INDEX games_name ON games(name);",
    // 23: which clone list a row came from, so a new version of the list
    // replaces its own rows (and old titles go).
    "ALTER TABLE clone_groups ADD COLUMN list TEXT NOT NULL DEFAULT '';",
    // 24: Wikidata editions ("Director's Cut") are candidates too; `work` is
    // the game an edition belongs to (P629), whose facts fill its gaps.
    "ALTER TABLE wikidata_candidates ADD COLUMN work TEXT;",
    // 25: games that are a folder of files, named by ScummVM's detection
    // list (milestone 14): worked out again after every scan.
    "CREATE TABLE folder_games (
         folder_id INTEGER PRIMARY KEY,
         path      TEXT NOT NULL UNIQUE,
         system_id INTEGER NOT NULL REFERENCES systems(system_id),
         title     TEXT NOT NULL,
         code      TEXT NOT NULL,
         files     INTEGER NOT NULL,
         size      INTEGER NOT NULL
     );",
    // 26: import journals the folder it makes for a set of loose disks
    // (`setdir`: undo removes it once empty). Rows keep their order.
    "CREATE TABLE cleanup_outputs_new (
         batch_id INTEGER NOT NULL REFERENCES cleanup_batches(batch_id),
         path     TEXT NOT NULL,
         kind     TEXT NOT NULL CHECK (kind IN ('file', 'dir', 'setdir')),
         PRIMARY KEY (batch_id, path)
     );
     INSERT INTO cleanup_outputs_new (batch_id, path, kind)
         SELECT batch_id, path, kind FROM cleanup_outputs ORDER BY rowid;
     DROP TABLE cleanup_outputs;
     ALTER TABLE cleanup_outputs_new RENAME TO cleanup_outputs;",
    // 27: folders ScummVM's own --detect named (its list leaves out
    // SCUMM, SCI, Sky and Glk), with a fingerprint of their files so the
    // name follows a folder that import copies or undo brings back.
    "CREATE TABLE scummvm_ids (
         path        TEXT PRIMARY KEY,
         fingerprint TEXT NOT NULL,
         code        TEXT NOT NULL,
         title       TEXT NOT NULL
     );
     CREATE INDEX scummvm_ids_fingerprint ON scummvm_ids(fingerprint);",
    // 28: when a batch's originals were deleted from the trash (milestone
    // 16); such a batch can no longer be undone.
    "ALTER TABLE cleanup_batches ADD COLUMN emptied_at TEXT;",
    // 29: which copy owns each ROM (milestone 16): the one whose archive
    // or folder holds the most of the ROM's game. A track two discs share
    // is then each disc's own copy. Set by `identify`; until the next scan,
    // a matched file owns its hash matches, as before.
    "ALTER TABLE matches ADD COLUMN owns INTEGER NOT NULL DEFAULT 0;
     UPDATE matches SET owns = 1
       WHERE out_of_scope = 0 AND via IN ('sha1', 'md5')
         AND file_id IN (SELECT file_id FROM files WHERE status = 'matched');",
    // 30: arcade (milestone 17). A DAT's disks (MAME's `<disk>`) are ROMs
    // matched by the SHA1 a CHD keeps in its header, read for every CHD;
    // CHDs scanned before are read again for it.
    "ALTER TABLE roms ADD COLUMN disk INTEGER NOT NULL DEFAULT 0;
     ALTER TABLE files ADD COLUMN chd_sha1 TEXT;
     CREATE INDEX files_chd_sha1 ON files(chd_sha1);
     UPDATE files SET mtime = NULL WHERE lower(path) GLOB '*.chd';",
    // 31: what romforge patched (milestone 19): the new file (a ROM, or a
    // disc's patched track) by content, the game and the patch.
    "CREATE TABLE patched (
         sha1       TEXT NOT NULL,
         size       INTEGER NOT NULL,
         base       TEXT NOT NULL,
         system_id  INTEGER NOT NULL REFERENCES systems(system_id),
         patch_name TEXT NOT NULL,
         patch_sha1 TEXT NOT NULL,
         kind       TEXT NOT NULL,
         created_at TEXT NOT NULL,
         PRIMARY KEY (sha1, size)
     );",
    // 32: patch files a scan found (loose, or the one patch in a zip), with
    // the game each is for; the patch that proved a file is a patched copy
    // of its game, when romforge named it so; and the files whose such
    // name the owner took off, never named again.
    "CREATE TABLE patch_files (
         path         TEXT PRIMARY KEY,
         root_id      INTEGER NOT NULL REFERENCES scan_roots(root_id) ON DELETE CASCADE,
         name         TEXT NOT NULL,
         kind         TEXT NOT NULL,
         sha1         TEXT NOT NULL,
         size         INTEGER NOT NULL,
         mtime        INTEGER,
         source_size  INTEGER,
         source_crc32 TEXT,
         target_size  INTEGER,
         target_crc32 TEXT,
         names_source TEXT,
         hint         TEXT NOT NULL,
         readme       TEXT,
         relation     TEXT NOT NULL,
         system_id    INTEGER REFERENCES systems(system_id),
         title        TEXT,
         paired_by    TEXT
     );
     CREATE INDEX patch_files_game ON patch_files(system_id, title);
     ALTER TABLE manual_ids ADD COLUMN proof TEXT;
     CREATE TABLE proofs_declined (
         sha1 TEXT NOT NULL,
         size INTEGER NOT NULL,
         PRIMARY KEY (sha1, size)
     );",
    // 33: who made each game, as Wikidata items (studios and people), and
    // each item's picture: a studio's logo, a person's photo, from
    // Wikimedia Commons, with where the cached copy is.
    "CREATE TABLE credits (
         system_id INTEGER NOT NULL REFERENCES systems(system_id),
         title     TEXT NOT NULL,
         field     TEXT NOT NULL,
         qid       TEXT NOT NULL,
         label     TEXT NOT NULL,
         PRIMARY KEY (system_id, title, field, qid)
     );
     CREATE TABLE wikidata_pictures (
         qid        TEXT PRIMARY KEY,
         url        TEXT,
         kind       TEXT,
         path       TEXT,
         checked_at TEXT NOT NULL
     );",
    // 34: where each picture was downloaded from, and whether the owner
    // picked it (Details' art picker); a picked one is never replaced by
    // Enrich, even when refreshing.
    "ALTER TABLE art ADD COLUMN url TEXT;
     ALTER TABLE art ADD COLUMN chosen INTEGER NOT NULL DEFAULT 0;",
    // 35: nothing for a new library. (A development build briefly kept a
    // generation counter here with triggers on the list's tables; they
    // slowed a big scan threefold, so the list is now keyed by a
    // fingerprint of those tables instead: `view::fingerprint`.)
    "DROP TRIGGER IF EXISTS view_files_ins;
     DROP TRIGGER IF EXISTS view_files_del;
     DROP TRIGGER IF EXISTS view_files_upd;
     DROP TRIGGER IF EXISTS view_matches_ins;
     DROP TRIGGER IF EXISTS view_matches_del;
     DROP TRIGGER IF EXISTS view_matches_upd;
     DROP TRIGGER IF EXISTS view_manual_ids_ins;
     DROP TRIGGER IF EXISTS view_manual_ids_del;
     DROP TRIGGER IF EXISTS view_manual_ids_upd;
     DROP TRIGGER IF EXISTS view_patch_files_ins;
     DROP TRIGGER IF EXISTS view_patch_files_del;
     DROP TRIGGER IF EXISTS view_patch_files_upd;
     DROP TRIGGER IF EXISTS view_folder_games_ins;
     DROP TRIGGER IF EXISTS view_folder_games_del;
     DROP TRIGGER IF EXISTS view_folder_games_upd;
     DROP TRIGGER IF EXISTS view_art_ins;
     DROP TRIGGER IF EXISTS view_art_del;
     DROP TRIGGER IF EXISTS view_art_upd;
     DROP TRIGGER IF EXISTS view_game_meta_ins;
     DROP TRIGGER IF EXISTS view_game_meta_del;
     DROP TRIGGER IF EXISTS view_game_meta_upd;
     DROP TRIGGER IF EXISTS view_clone_groups_ins;
     DROP TRIGGER IF EXISTS view_clone_groups_del;
     DROP TRIGGER IF EXISTS view_clone_groups_upd;
     DROP TRIGGER IF EXISTS view_dats_ins;
     DROP TRIGGER IF EXISTS view_dats_del;
     DROP TRIGGER IF EXISTS view_dats_upd;
     DROP TRIGGER IF EXISTS view_settings_ins;
     DROP TRIGGER IF EXISTS view_settings_upd;
     DROP TRIGGER IF EXISTS view_settings_del;
     DROP TABLE IF EXISTS view_generation;",
];
