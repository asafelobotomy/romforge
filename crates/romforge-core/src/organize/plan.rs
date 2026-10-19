use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::Error;

use super::apply::absolute_dest;
use super::names::{base_title, clone_groups, common_game, rank, sanitize};
use super::{GameRef, Member, Move, OrganizeOptions, Plan, Skip};

/// Whether `path` is under the source: `from` (canonical) when it is a
/// folder, else `from` itself.
fn in_source(path: &str, from: Option<&(PathBuf, bool)>) -> bool {
    let path = Path::new(path);
    match from {
        Some((from, true)) => path != from && path.starts_with(from),
        Some((from, false)) => path == from,
        None => false,
    }
}

pub(super) fn load(
    conn: &Connection,
    opts: &OrganizeOptions,
) -> Result<BTreeMap<String, BTreeMap<String, Member>>, Error> {
    let mut stmt = conn.prepare(concat!(
        "SELECT f.path, f.member, f.status, coalesce(s.key, d.name), g.name, r.name,
                (SELECT count(*) FROM roms r2 WHERE r2.game_id = g.game_id),
                (SELECT count(*) FROM roms r3 WHERE r3.game_id = g.game_id
                   AND (lower(r3.name) LIKE '%.cue' OR lower(r3.name) LIKE '%.gdi'))
         FROM files f
         LEFT JOIN matches m ON m.file_id = f.file_id AND m.out_of_scope = 0
              AND m.via IN ",
        crate::report::strong!(),
        "
         LEFT JOIN roms r ON r.rom_id = m.rom_id
         LEFT JOIN games g ON g.game_id = r.game_id
         LEFT JOIN dats d ON d.dat_id = g.dat_id
         LEFT JOIN systems s ON s.system_id = d.system_id
         ORDER BY f.path, f.member"
    ))?;
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

    let source = std::fs::canonicalize(&opts.from)
        .ok()
        .map(|f| (f.clone(), f.is_dir()));
    for (path, members) in &files {
        if !in_source(path, source.as_ref()) {
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
        } else if from.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
            // Moving the link would leave its target behind; the target
            // is organized where it is scanned.
            plan.skipped.push(skip("a symbolic link".into()));
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
