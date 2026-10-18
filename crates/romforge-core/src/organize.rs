//! Milestone 2: rename and move identified files into a tidy tree, with a
//! dry run first, a journal of every move, and undo.
//!
//! Layout under the destination:
//! - an archive whose matched members all belong to one game:
//!   `<system>/<game>.<zip|7z>` (the archive itself is never rewritten);
//! - a loose file of a one-ROM game: `<system>/<rom name>`;
//! - loose files of a multi-ROM game (discs): `<system>/<game>/<rom name>`;
//!   a disc whose cue sheet differs from the DAT's moves only with its own
//!   sheet, which must list exactly its matched tracks, and keeps its names.
//!
//! Names come from the DAT; a file keeps its own extension. Only `matched`
//! files move. Nothing is ever overwritten. A move is a hard link plus
//! delete, a rename, or (across drives) a copy that is checked byte for byte
//! before the original is deleted.
//!
//! 1G1R ("one game, one ROM") keeps one version of each title: titles are
//! grouped by name without `(...)`/`[...]` tags, and the version with the
//! best region, then the highest revision, wins; betas, protos, demos and
//! the like lose to any release.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use sha1::{Digest as _, Sha1};

use crate::Error;

/// What [`plan`] should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizeOptions {
    /// Only files under this folder move. Required: the library may hold
    /// scans of folders you never meant to reorganize.
    pub from: PathBuf,
    pub dest: PathBuf,
    /// Only files identified for this system key.
    pub system: Option<String>,
    /// Keep one version per title.
    pub one_g1r: bool,
    /// Region preference for 1G1R, best first.
    pub regions: Vec<String>,
}

pub const DEFAULT_REGIONS: &[&str] = &["USA", "World", "Europe", "Japan"];

impl OrganizeOptions {
    pub fn new(from: impl Into<PathBuf>, dest: impl Into<PathBuf>) -> Self {
        Self {
            from: from.into(),
            dest: dest.into(),
            system: None,
            one_g1r: false,
            regions: DEFAULT_REGIONS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Move {
    pub from: PathBuf,
    pub to: PathBuf,
    pub system: String,
    pub game: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Skip {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Plan {
    pub moves: Vec<Move>,
    pub skipped: Vec<Skip>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Applied {
    pub batch_id: i64,
    pub moved: usize,
    pub failed: Vec<(Move, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Undone {
    pub batch_id: i64,
    pub restored: usize,
    pub failed: Vec<(PathBuf, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Batch {
    pub batch_id: i64,
    pub created_at: String,
    pub dest: String,
    pub moves: i64,
    pub undone_at: Option<String>,
}

/// A game a file was identified as.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct GameRef {
    system: String,
    game: String,
}

struct Member {
    status: String,
    /// game -> (ROM name in the DAT, ROM count of the game, game has a cue)
    games: BTreeMap<GameRef, (String, i64, bool)>,
}

/// Whether `path` is under the source (`from`) of `opts`.
fn in_source(path: &str, opts: &OrganizeOptions) -> bool {
    let Ok(from) = std::fs::canonicalize(&opts.from) else {
        return false;
    };
    if from.is_dir() {
        path.starts_with(&format!("{}/", from.display()))
    } else {
        Path::new(path) == from
    }
}

fn load(
    conn: &Connection,
    opts: &OrganizeOptions,
) -> Result<BTreeMap<String, BTreeMap<String, Member>>, Error> {
    let mut stmt = conn.prepare(
        "SELECT f.path, f.member, f.status, coalesce(s.key, d.name), g.name, r.name,
                (SELECT count(*) FROM roms r2 WHERE r2.game_id = g.game_id),
                (SELECT count(*) FROM roms r3 WHERE r3.game_id = g.game_id
                   AND lower(r3.name) LIKE '%.cue')
         FROM files f
         LEFT JOIN matches m ON m.file_id = f.file_id AND m.out_of_scope = 0
              AND m.via IN ('sha1', 'md5')
         LEFT JOIN roms r ON r.rom_id = m.rom_id
         LEFT JOIN games g ON g.game_id = r.game_id
         LEFT JOIN dats d ON d.dat_id = g.dat_id
         LEFT JOIN systems s ON s.system_id = d.system_id
         ORDER BY f.path, f.member",
    )?;
    let mut out: BTreeMap<String, BTreeMap<String, Member>> = BTreeMap::new();
    let dest = format!("{}/", opts.dest.display());
    let from = std::fs::canonicalize(&opts.from)
        .map_err(|e| Error::Organize(format!("source folder {}: {e}", opts.from.display())))?;
    // A file as the source means just that file.
    let (from, exact) = if from.is_dir() {
        (format!("{}/", from.display()), None)
    } else {
        (String::new(), Some(from.to_string_lossy().into_owned()))
    };
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, i64>(7)?,
        ))
    })?;
    for row in rows {
        let (path, member, status, system, game, rom, rom_count, cues) = row?;
        let inside = match &exact {
            Some(file) => path == *file,
            None => path.starts_with(from.as_str()),
        };
        // Files in the destination are loaded too, to know what is already
        // organized; only files under `from` move.
        if !inside && !path.starts_with(dest.as_str()) {
            continue;
        }
        let entry = out
            .entry(path)
            .or_default()
            .entry(member)
            .or_insert_with(|| Member {
                status,
                games: BTreeMap::new(),
            });
        if let (Some(system), Some(game), Some(rom)) = (system, game, rom) {
            if let Some(want) = &opts.system
                && !want.eq_ignore_ascii_case(&system)
            {
                continue;
            }
            entry
                .games
                .entry(GameRef { system, game })
                .or_insert((rom, rom_count, cues > 0));
        }
    }
    Ok(out)
}

/// Work out the moves (nothing is touched). Every file that won't move is
/// listed in `skipped` with the reason.
pub fn plan(conn: &Connection, opts: &OrganizeOptions) -> Result<Plan, Error> {
    let opts = &OrganizeOptions {
        dest: absolute_dest(&opts.dest),
        ..opts.clone()
    };
    let files = load(conn, opts)?;
    let mut plan = Plan::default();
    // Proposed moves before collision checks: (from, game, target)
    let mut proposed: Vec<(PathBuf, GameRef, PathBuf)> = Vec::new();
    // Loose files per game, to check disc sets are complete.
    let mut loose_by_game: HashMap<GameRef, Vec<usize>> = HashMap::new();
    let mut loose_cue_games: HashMap<GameRef, i64> = HashMap::new();

    // Games that already have a file in the destination: their other copies
    // stay where they are (which copy counts as the duplicate follows path
    // order, so it can change once files have moved).
    let dest_prefix = format!("{}/", opts.dest.display());
    let mut organized: HashMap<GameRef, PathBuf> = HashMap::new();
    for (path, members) in &files {
        if !path.starts_with(&dest_prefix) {
            continue;
        }
        let kept: Vec<&Member> = members
            .values()
            .filter(|m| m.status == "matched" || m.status == "duplicate")
            .collect();
        let is_archive = members.keys().any(|m| !m.is_empty());
        if let Ok(game) = common_game(&kept, is_archive) {
            organized.entry(game).or_insert_with(|| PathBuf::from(path));
        }
    }

    for (path, members) in &files {
        if !in_source(path, opts) {
            continue;
        }
        let from = PathBuf::from(path);
        let skip = |reason: String| Skip {
            path: from.clone(),
            reason,
        };
        let is_archive = members.keys().any(|m| !m.is_empty());
        let matched: Vec<&Member> = members.values().filter(|m| m.status == "matched").collect();
        if matched.is_empty() {
            let statuses: BTreeSet<&str> = members.values().map(|m| m.status.as_str()).collect();
            let statuses: Vec<&str> = statuses.into_iter().collect();
            plan.skipped
                .push(skip(format!("not matched ({})", statuses.join(", "))));
            continue;
        }
        if opts.system.is_some() && matched.iter().all(|m| m.games.is_empty()) {
            continue;
        }
        let game = match common_game(&matched, is_archive) {
            Ok(g) => g,
            Err(reason) => {
                plan.skipped.push(skip(reason));
                continue;
            }
        };
        if let Some(there) = organized.get(&game)
            && *there != from
        {
            plan.skipped.push(skip(format!(
                "{} is already organized at {}",
                game.game,
                there.display()
            )));
            continue;
        }
        let system_dir = opts.dest.join(sanitize(&game.system));
        let ext = from.extension().map(|e| e.to_string_lossy().into_owned());
        let with_ext = |stem: &str| match &ext {
            Some(e) => format!("{stem}.{e}"),
            None => stem.to_owned(),
        };
        let target = if is_archive {
            system_dir.join(sanitize(&with_ext(&game.game)))
        } else {
            let (rom, rom_count, has_cue) = &matched[0].games[&game];
            let stem = Path::new(rom)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| rom.clone());
            let name = sanitize(&with_ext(&stem));
            if *has_cue {
                loose_cue_games.insert(game.clone(), *rom_count);
            }
            if *rom_count > 1 {
                system_dir.join(sanitize(&game.game)).join(name)
            } else {
                system_dir.join(name)
            }
        };
        if !is_archive {
            loose_by_game
                .entry(game.clone())
                .or_default()
                .push(proposed.len());
        }
        proposed.push((from, game, target));
    }

    // Loose disc sets move only when complete: renaming tracks without their
    // cue sheet (or with one that differs) would break the sheet. A set
    // whose own sheet (not the DAT's) lists exactly its matched tracks moves
    // as is, keeping every file name, so the sheet still works.
    let mut dropped: HashSet<usize> = HashSet::new();
    let mut extra: Vec<(PathBuf, GameRef, PathBuf)> = Vec::new();
    for (game, rom_count) in &loose_cue_games {
        let idx = &loose_by_game[game];
        if (idx.len() as i64) >= *rom_count {
            continue;
        }
        let tracks: BTreeSet<&PathBuf> = idx.iter().map(|&i| &proposed[i].0).collect();
        let own_sheet = tracks
            .iter()
            .filter_map(|p| p.parent())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .flat_map(|dir| {
                files
                    .iter()
                    .filter(move |(p, m)| {
                        Path::new(p.as_str()).parent() == Some(dir)
                            && m.values().all(|e| e.status != "matched")
                    })
                    .map(|(p, _)| PathBuf::from(p))
            })
            .find(|sheet| match crate::disc::sheet_files(sheet) {
                Some(Ok(listed)) => listed.iter().collect::<BTreeSet<_>>() == tracks,
                _ => false,
            });
        match own_sheet {
            Some(sheet) => {
                let folder = opts.dest.join(sanitize(&game.system)).join(sanitize(&game.game));
                for &i in idx {
                    let name = proposed[i].0.file_name().unwrap_or_default().to_owned();
                    proposed[i].2 = folder.join(name);
                }
                let name = sheet.file_name().unwrap_or_default().to_owned();
                extra.push((sheet.clone(), game.clone(), folder.join(name)));
                plan.skipped.retain(|s| s.path != sheet);
            }
            None => {
                for &i in idx {
                    dropped.insert(i);
                    plan.skipped.push(Skip {
                        path: proposed[i].0.clone(),
                        reason: format!(
                            "disc set of {} incomplete or its cue sheet differs ({} of {} files matched); \
                             moving it would break the cue sheet",
                            game.game,
                            idx.len(),
                            rom_count
                        ),
                    });
                }
            }
        }
    }
    proposed.extend(extra);

    // 1G1R: one version per (system, title), counting versions already
    // organized in the destination: a worse version never joins a better one.
    // Titles a clone list groups (one game under other names) are one title.
    if opts.one_g1r {
        let clones = clone_groups(conn)?;
        let title_of = |system: &str, game: &str| -> String {
            let base = base_title(game);
            match clones.get(&(system.to_owned(), base.clone())) {
                Some((grp, _)) => format!("group:{}", grp.to_lowercase()),
                None => base,
            }
        };
        let priority = |system: &str, game: &str| -> i64 {
            clones
                .get(&(system.to_owned(), base_title(game)))
                .map_or(1, |(_, p)| *p)
        };
        let mut groups: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
        for (i, (_, game, _)) in proposed.iter().enumerate() {
            if !dropped.contains(&i) {
                groups
                    .entry((game.system.clone(), title_of(&game.system, &game.game)))
                    .or_default()
                    .push(i);
            }
        }
        let mut already: BTreeMap<(String, String), Vec<&str>> = BTreeMap::new();
        for g in organized.keys() {
            already
                .entry((g.system.clone(), title_of(&g.system, &g.game)))
                .or_default()
                .push(g.game.as_str());
        }
        for (key, idx) in &groups {
            let mut names: BTreeSet<&str> = idx.iter().map(|&i| proposed[i].1.game.as_str()).collect();
            names.extend(already.get(key).into_iter().flatten().copied());
            if names.len() < 2 {
                continue;
            }
            let best = names
                .iter()
                .copied()
                .max_by_key(|n| {
                    let (retail, region, revision) = rank(n, &opts.regions);
                    (retail, region, -priority(&key.0, n), revision)
                })
                .unwrap()
                .to_owned();
            for &i in idx {
                if proposed[i].1.game != best {
                    dropped.insert(i);
                    plan.skipped.push(Skip {
                        path: proposed[i].0.clone(),
                        reason: format!("1G1R: keeping {best}"),
                    });
                }
            }
        }
    }

    let mut taken: HashSet<PathBuf> = HashSet::new();
    for (i, (from, game, to)) in proposed.into_iter().enumerate() {
        if dropped.contains(&i) {
            continue;
        }
        let skip = |reason: String| Skip {
            path: from.clone(),
            reason,
        };
        if from == to {
            plan.skipped.push(skip("already in place".into()));
        } else if !taken.insert(to.clone()) {
            plan.skipped
                .push(skip(format!("another file is going to {}", to.display())));
        } else if to.symlink_metadata().is_ok() {
            plan.skipped
                .push(skip(format!("{} already exists", to.display())));
        } else {
            plan.moves.push(Move {
                from,
                to,
                system: game.system,
                game: game.game,
            });
        }
    }
    plan.skipped.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(plan)
}

/// The one game all of a path's matched entries agree on, or why there
/// isn't one.
fn common_game(matched: &[&Member], is_archive: bool) -> Result<GameRef, String> {
    let mut common: Option<BTreeSet<&GameRef>> = None;
    for m in matched {
        let games: BTreeSet<&GameRef> = m.games.keys().collect();
        common = Some(match common {
            None => games,
            Some(c) => c.intersection(&games).copied().collect(),
        });
    }
    let common = common.unwrap_or_default();
    match common.len() {
        0 if is_archive => {
            Err("holds ROMs of different games (splitting an archive would rewrite it)".into())
        }
        0 => Err("matched in no single game".into()),
        1 => Ok((*common.iter().next().unwrap()).clone()),
        _ => {
            let names: Vec<String> = common.iter().take(3).map(|g| g.game.clone()).collect();
            Err(format!("ambiguous: {}", names.join(" | ")))
        }
    }
}

/// A name safe as one path component on Linux, macOS and Windows.
pub(crate) fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let s = s.trim().trim_end_matches('.').trim();
    if s.is_empty() || s == "." || s == ".." {
        "_".into()
    } else {
        s.to_owned()
    }
}

/// The title without `(...)` and `[...]` tags: `Super Mario World (USA)`
/// and `Super Mario World (Europe) (Rev 1)` are one title.
pub fn base_title(name: &str) -> String {
    let mut out = String::new();
    let mut depth = 0i32;
    for c in name.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// (system key, title without tags) -> (clone group, priority), from the
/// clone lists `dat download` stored.
type CloneGroups = HashMap<(String, String), (String, i64)>;

fn clone_groups(conn: &Connection) -> Result<CloneGroups, Error> {
    Ok(conn
        .prepare(
            "SELECT s.key, c.title, c.grp, c.priority FROM clone_groups c
             JOIN systems s ON s.system_id = c.system_id",
        )?
        .query_map([], |r| Ok(((r.get(0)?, r.get(1)?), (r.get(2)?, r.get(3)?))))?
        .collect::<Result<_, _>>()?)
}

/// How good a release a DAT name is, for 1G1R: retail first, then the
/// best region in `regions`, then the highest revision ([`names::parse`]).
fn rank(name: &str, regions: &[String]) -> (bool, i64, Vec<u64>) {
    let n = crate::names::parse(name);
    let region = n
        .regions
        .iter()
        .filter_map(|r| regions.iter().position(|w| w.eq_ignore_ascii_case(r)))
        .min()
        .map_or(-1, |p| (regions.len() - p) as i64);
    (n.is_retail(), region, n.revision())
}

/// Carry out a plan, journaling each move so [`undo`] can reverse it. A
/// plan that moves nothing leaves no batch behind (`batch_id` 0), so `undo`
/// never lands on an empty batch.
pub fn apply(conn: &mut Connection, plan: &Plan, dest: &Path) -> Result<Applied, Error> {
    let mut applied = Applied::default();
    if plan.moves.is_empty() {
        return Ok(applied);
    }
    let dest = absolute_dest(dest);
    conn.execute(
        "INSERT INTO organize_batches (created_at, dest)
         VALUES (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), ?1)",
        [dest.to_string_lossy()],
    )?;
    let batch_id = conn.last_insert_rowid();
    for (seq, m) in plan.moves.iter().enumerate() {
        match move_file(&m.from, &m.to) {
            Ok(size) => {
                let tx = conn.transaction()?;
                tx.execute(
                    "INSERT INTO organize_moves (batch_id, seq, src, dst, size) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        batch_id,
                        seq as i64,
                        m.from.to_string_lossy(),
                        m.to.to_string_lossy(),
                        size
                    ],
                )?;
                follow(&tx, &m.from, &m.to, &dest)?;
                tx.commit()?;
                applied.moved += 1;
            }
            Err(e) => applied.failed.push((m.clone(), e.to_string())),
        }
    }
    if applied.moved == 0 {
        conn.execute("DELETE FROM organize_batches WHERE batch_id = ?1", [batch_id])?;
    } else {
        applied.batch_id = batch_id;
    }
    Ok(applied)
}

/// The library follows a moved file: its rows get the new path and move to
/// the scan root that contains it (`new_root` is added as one if none
/// does), so rescanning the folder it came from doesn't drop it.
fn follow(tx: &rusqlite::Transaction, from: &Path, to: &Path, new_root: &Path) -> Result<(), Error> {
    let roots: Vec<(i64, String)> = tx
        .prepare("SELECT root_id, path FROM scan_roots")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let root = roots
        .iter()
        .filter(|(_, p)| to.starts_with(p))
        .max_by_key(|(_, p)| p.len())
        .map(|(id, _)| *id);
    let root = match root {
        Some(id) => id,
        None => {
            let dir = new_root;
            tx.query_row(
                "INSERT INTO scan_roots (path, scanned_at)
                 VALUES (?1, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')) RETURNING root_id",
                [dir.to_string_lossy()],
                |r| r.get(0),
            )?
        }
    };
    tx.execute(
        "UPDATE files SET path = ?2, root_id = ?3 WHERE path = ?1",
        params![from.to_string_lossy(), to.to_string_lossy(), root],
    )?;
    Ok(())
}

fn absolute_dest(dest: &Path) -> PathBuf {
    std::fs::canonicalize(dest)
        .or_else(|_| std::path::absolute(dest))
        .unwrap_or_else(|_| dest.to_path_buf())
}

/// Move the files of a batch (the latest one not yet undone, by default)
/// back where they were, newest first. A file that changed size since, or
/// whose old place is taken, stays put and is reported.
pub fn undo(conn: &mut Connection, batch_id: Option<i64>) -> Result<Undone, Error> {
    let batch_id = match batch_id {
        Some(id) => conn
            .query_row(
                "SELECT batch_id FROM organize_batches WHERE batch_id = ?1 AND undone_at IS NULL",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::Organize(format!("no batch {id} to undo")))?,
        None => conn
            .query_row(
                "SELECT max(batch_id) FROM organize_batches WHERE undone_at IS NULL",
                [],
                |r| r.get::<_, Option<i64>>(0),
            )?
            .ok_or_else(|| Error::Organize("nothing to undo".into()))?,
    };
    let dest: PathBuf = conn
        .query_row(
            "SELECT dest FROM organize_batches WHERE batch_id = ?1",
            [batch_id],
            |r| r.get::<_, String>(0),
        )?
        .into();
    let moves: Vec<(i64, String, String, i64)> = conn
        .prepare("SELECT seq, src, dst, size FROM organize_moves WHERE batch_id = ?1 ORDER BY seq DESC")?
        .query_map([batch_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    let mut undone = Undone {
        batch_id,
        ..Undone::default()
    };
    for (seq, src, dst, size) in moves {
        let (src, dst) = (PathBuf::from(src), PathBuf::from(dst));
        let now = std::fs::metadata(&dst).map(|m| m.len() as i64);
        let result = match now {
            Ok(n) if n != size => Err(format!("{} changed size since it was moved", dst.display())),
            Ok(_) => move_file(&dst, &src).map(drop).map_err(|e| e.to_string()),
            Err(e) => Err(format!("{}: {e}", dst.display())),
        };
        match result {
            Ok(()) => {
                let tx = conn.transaction()?;
                tx.execute(
                    "DELETE FROM organize_moves WHERE batch_id = ?1 AND seq = ?2",
                    params![batch_id, seq],
                )?;
                follow(&tx, &dst, &src, src.parent().unwrap_or(&src))?;
                tx.commit()?;
                remove_empty_dirs(&dst, &dest);
                undone.restored += 1;
            }
            Err(e) => undone.failed.push((dst, e)),
        }
    }
    if undone.failed.is_empty() {
        conn.execute(
            "UPDATE organize_batches SET undone_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE batch_id = ?1",
            [batch_id],
        )?;
    }
    Ok(undone)
}

pub fn batches(conn: &Connection) -> Result<Vec<Batch>, Error> {
    let mut stmt = conn.prepare(
        "SELECT b.batch_id, b.created_at, b.dest, b.undone_at,
                (SELECT count(*) FROM organize_moves m WHERE m.batch_id = b.batch_id)
         FROM organize_batches b ORDER BY b.batch_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Batch {
            batch_id: r.get(0)?,
            created_at: r.get(1)?,
            dest: r.get(2)?,
            undone_at: r.get(3)?,
            moves: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Folders under `dest` that undo left empty, up to the first that still
/// has something in it. Never `dest` itself or anything outside it, and only
/// ever empty folders.
fn remove_empty_dirs(moved_out_of: &Path, dest: &Path) {
    let mut dir = moved_out_of.parent();
    while let Some(d) = dir {
        if d == dest || !d.starts_with(dest) || std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// Move `from` to `to` without ever replacing an existing file. Returns the
/// size moved.
pub(crate) fn move_file(from: &Path, to: &Path) -> std::io::Result<u64> {
    use std::io::{Error as IoError, ErrorKind};
    let size = std::fs::symlink_metadata(from)?.len();
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // A hard link fails if `to` exists, so nothing is clobbered even if a
    // file appears there meanwhile.
    match std::fs::hard_link(from, to) {
        Ok(()) => {
            std::fs::remove_file(from)?;
            return Ok(size);
        }
        Err(e) if e.kind() == ErrorKind::AlreadyExists => return Err(e),
        Err(e) if e.kind() == ErrorKind::CrossesDevices => return copy_verify_delete(from, to, size),
        Err(_) => {}
    }
    // Filesystems without hard links (FAT, exFAT, some network shares).
    if to.symlink_metadata().is_ok() {
        return Err(IoError::new(
            ErrorKind::AlreadyExists,
            format!("{} exists", to.display()),
        ));
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(size),
        Err(e) if e.kind() == ErrorKind::CrossesDevices => copy_verify_delete(from, to, size),
        Err(e) => Err(e),
    }
}

fn copy_verify_delete(from: &Path, to: &Path, size: u64) -> std::io::Result<u64> {
    use std::io::{Error as IoError, ErrorKind};
    let name = to
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = to.with_file_name(format!(".{name}.romforge-part"));
    let result = (|| {
        {
            let mut src = std::fs::File::open(from)?;
            let meta = src.metadata()?;
            let mut out = std::fs::File::create_new(&tmp)?;
            std::io::copy(&mut src, &mut out)?;
            // Keep the file's times and permissions, as a rename would.
            let mut times = std::fs::FileTimes::new().set_modified(meta.modified()?);
            if let Ok(accessed) = meta.accessed() {
                times = times.set_accessed(accessed);
            }
            out.set_times(times)?;
            out.set_permissions(meta.permissions())?;
            out.sync_all()?;
        }
        if sha1_file(from)? != sha1_file(&tmp)? || std::fs::metadata(&tmp)?.len() != size {
            return Err(IoError::other("copy does not match the original"));
        }
        if to.symlink_metadata().is_ok() {
            return Err(IoError::new(
                ErrorKind::AlreadyExists,
                format!("{} exists", to.display()),
            ));
        }
        std::fs::rename(&tmp, to)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::remove_file(from)?;
    Ok(size)
}

fn sha1_file(path: &Path) -> std::io::Result<[u8; 20]> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha1::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regions() -> Vec<String> {
        DEFAULT_REGIONS.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn titles_group_without_tags() {
        assert_eq!(base_title("Super Mario World (USA)"), "super mario world");
        assert_eq!(
            base_title("Super Mario World (Europe) (Rev 1) [!]"),
            "super mario world"
        );
        assert_ne!(base_title("Super Mario World 2 (USA)"), "super mario world");
    }

    #[test]
    fn rank_prefers_release_region_then_revision() {
        let r = regions();
        let best = |names: &[&'static str]| -> &'static str {
            names.iter().copied().max_by_key(|n| rank(n, &r)).unwrap()
        };
        assert_eq!(best(&["X (Japan)", "X (USA)", "X (Europe)"]), "X (USA)");
        assert_eq!(
            best(&["X (USA)", "X (USA) (Rev 2)", "X (USA) (Rev 1)"]),
            "X (USA) (Rev 2)"
        );
        assert_eq!(best(&["X (USA) (Beta)", "X (Japan)"]), "X (Japan)");
        assert_eq!(best(&["X (USA, Europe)", "X (Europe)"]), "X (USA, Europe)");
        assert_eq!(best(&["X (Brazil)", "X (Japan)"]), "X (Japan)");
    }

    #[test]
    fn copy_verify_delete_moves_and_never_clobbers() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.bin"), dir.path().join("sub/b.bin"));
        std::fs::write(&a, vec![5u8; 100_000]).unwrap();
        std::fs::create_dir_all(b.parent().unwrap()).unwrap();
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        std::fs::File::options()
            .write(true)
            .open(&a)
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert_eq!(copy_verify_delete(&a, &b, 100_000).unwrap(), 100_000);
        assert_eq!(
            std::fs::metadata(&b).unwrap().modified().unwrap(),
            old,
            "mtime kept"
        );
        assert!(!a.exists());
        assert_eq!(std::fs::read(&b).unwrap(), vec![5u8; 100_000]);
        let leftovers: Vec<_> = std::fs::read_dir(b.parent().unwrap()).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "no .romforge-part left behind");

        std::fs::write(&a, b"new").unwrap();
        assert!(copy_verify_delete(&a, &b, 3).is_err(), "b exists");
        assert!(a.exists(), "the original stays when the move fails");
        assert!(move_file(&a, &b).is_err());
        assert_eq!(std::fs::read(&b).unwrap().len(), 100_000);
    }

    #[test]
    fn sanitize_names() {
        assert_eq!(sanitize("A/B: C?"), "A-B- C-");
        assert_eq!(sanitize(" .. "), "_");
        assert_eq!(sanitize("Game."), "Game");
    }
}
