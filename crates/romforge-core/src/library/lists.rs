//! Bringing the lists games are identified by up to date, as Refresh does
//! before it scans: so a game is named (and a ScummVM game's files passed
//! over) in the same Refresh, not the one after.
//!
//! - Lists made by programs you have installed (ScummVM's detection list,
//!   MAME's own DAT) are read again when that program changed (its version
//!   differs from the one that made the list) or the list is missing. MAME
//!   only for a library that has Arcade (its list is large).
//! - Online lists (DATs, serials, clone lists; libretro's ScummVM names)
//!   only for systems you have files for, once they are older than
//!   Settings' `list_age_days` (0: never).
//!
//! What was checked is kept in `settings` (`list:…` keys).

use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension, params};

use crate::job::{Cancel, Step};
use crate::{Error, Library, fetch, launch, systems};

mod local;

/// What a lists step did.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ListsReport {
    /// What was read or downloaded ("ScummVM's list (ScummVM 2026.3.0)",
    /// "DATs: SNES, Genesis").
    pub updated: Vec<String>,
    /// What failed; a failed list never stops the Refresh.
    pub problems: Vec<String>,
}

/// How a lists step runs.
#[derive(Debug, Clone, Default)]
pub struct ListOptions {
    /// Online lists older than this many days are downloaded again; 0:
    /// none are.
    pub max_age_days: u32,
    pub cancel: Cancel,
    /// Where to find ScummVM and MAME; `None`: [`Library::env`].
    pub env: Option<launch::Env>,
}

fn stamp(conn: &Connection, key: &str) -> Result<Option<String>, Error> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .optional()?)
}

fn set_stamp(conn: &Connection, key: &str, value: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Note now as when `key` was last checked.
fn checked_now(conn: &Connection, key: &str) -> Result<(), Error> {
    conn.execute(
        concat!(
            "INSERT INTO settings (key, value) VALUES (?1, ",
            crate::db::now!(),
            ") ON CONFLICT(key) DO UPDATE SET value = excluded.value"
        ),
        [key],
    )?;
    Ok(())
}

/// Whether `key` was checked less than `days` days before `now` (a
/// timestamp as romforge stores them).
fn fresh(conn: &Connection, key: &str, days: u32, now: &str) -> Result<bool, Error> {
    let Some(at) = stamp(conn, key)? else {
        return Ok(false);
    };
    Ok(conn.query_row(
        "SELECT julianday(?1) - julianday(?2) < ?3",
        params![now, at, days],
        |r| r.get::<_, bool>(0),
    )?)
}

fn now(conn: &Connection) -> Result<String, Error> {
    Ok(conn.query_row(concat!("SELECT ", crate::db::now!()), [], |r| r.get(0))?)
}

/// The systems you have files for (keys): files matched to a game, named
/// by you, ScummVM games, folders scanned for one system, and files whose
/// extension only one system uses (before any of its DATs is in).
pub(crate) fn systems_in_use(conn: &Connection) -> Result<BTreeSet<&'static str>, Error> {
    let mut ids: BTreeSet<i64> = BTreeSet::new();
    for sql in [
        "SELECT DISTINCT d.system_id FROM matches m JOIN roms r ON r.rom_id = m.rom_id
           JOIN games g ON g.game_id = r.game_id JOIN dats d ON d.dat_id = g.dat_id
         WHERE d.system_id IS NOT NULL",
        "SELECT DISTINCT system_id FROM manual_ids",
        "SELECT DISTINCT system_id FROM folder_games",
        "SELECT DISTINCT system_id FROM scan_roots WHERE system_id IS NOT NULL",
    ] {
        for id in conn.prepare(sql)?.query_map([], |r| r.get::<_, i64>(0))? {
            ids.insert(id?);
        }
    }
    let mut out: BTreeSet<&'static str> = ids
        .into_iter()
        .filter_map(systems::by_id)
        .map(|s| s.key)
        .collect();
    let mut names = conn.prepare("SELECT path, member FROM files")?;
    let mut exts: BTreeSet<String> = BTreeSet::new();
    for row in names.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (path, member) = row?;
        let name = if member.is_empty() { path } else { member };
        if let Some((_, e)) = name.rsplit_once('.') {
            exts.insert(e.to_ascii_lowercase());
        }
    }
    out.extend(
        exts.iter()
            .filter_map(|e| systems::by_extension(e))
            .map(|s| s.key),
    );
    Ok(out)
}

/// Of `keys`, the systems whose online lists are due at `now`: they have
/// a source, and weren't checked in the last `days` days. None with 0.
pub(crate) fn due(
    conn: &Connection,
    keys: &BTreeSet<&'static str>,
    days: u32,
    now: &str,
) -> Result<Vec<&'static str>, Error> {
    if days == 0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for key in keys {
        let Some(s) = systems::by_key(key) else { continue };
        if fetch::sources(s, false).is_empty() {
            continue;
        }
        if !fresh(conn, &format!("list:checked:{key}"), days, now)? {
            out.push(s.key);
        }
    }
    Ok(out)
}

impl Library {
    /// Bring the lists up to date ([module docs](self)); `progress` gets
    /// how many of its steps are done.
    pub fn update_lists(
        &mut self,
        opts: &ListOptions,
        mut progress: impl FnMut(Step),
    ) -> Result<ListsReport, Error> {
        let mut report = ListsReport::default();
        let env = match &opts.env {
            Some(e) => e.clone(),
            None => self.env()?,
        };
        progress(Step::new(0, 0, "checking lists"));
        self.local_lists(&env, opts, &mut report, &mut progress)?;
        let keys = systems_in_use(&self.conn)?;
        self.online_lists(&keys, opts, &mut report, &mut progress)?;
        Ok(report)
    }

    /// After a scan: the online lists of systems it found first, and
    /// ScummVM's --detect of folders it never looked at.
    pub fn update_new_lists(
        &mut self,
        before: &BTreeSet<&'static str>,
        opts: &ListOptions,
        report: &mut ListsReport,
        mut progress: impl FnMut(Step),
    ) -> Result<(), Error> {
        let env = match &opts.env {
            Some(e) => e.clone(),
            None => self.env()?,
        };
        if launch::installed(&env, "scummvm") {
            self.detect_new_roots(&env, report);
        }
        let new: BTreeSet<&'static str> = systems_in_use(&self.conn)?
            .into_iter()
            .filter(|k| !before.contains(k))
            .collect();
        self.online_lists(&new, opts, report, &mut progress)
    }

    fn local_lists(
        &mut self,
        env: &launch::Env,
        opts: &ListOptions,
        report: &mut ListsReport,
        progress: &mut impl FnMut(Step),
    ) -> Result<(), Error> {
        let has_scummvm = launch::installed(env, "scummvm");
        if has_scummvm && !opts.cancel.is_set() {
            progress(Step::new(0, 0, "ScummVM's list"));
            self.scummvm_list_if_changed(env, report);
        }
        let titles_old = opts.max_age_days > 0
            && !fresh(
                &self.conn,
                "list:scummvm-titles",
                opts.max_age_days,
                &now(&self.conn)?,
            )?;
        let games: i64 = self
            .conn
            .query_row("SELECT count(*) FROM folder_games", [], |r| r.get(0))?;
        if titles_old && (has_scummvm || games > 0) && !opts.cancel.is_set() {
            progress(Step::new(0, 0, "libretro's ScummVM names"));
            match self.scummvm_titles_download() {
                Ok(n) => {
                    checked_now(&self.conn, "list:scummvm-titles")?;
                    report.updated.push(format!("libretro's ScummVM names ({n})"));
                }
                Err(e) => report.problems.push(format!("libretro's ScummVM names: {e}")),
            }
        }
        if launch::installed(env, "mame") && !opts.cancel.is_set() {
            progress(Step::new(0, 0, "MAME's list"));
            self.mame_list_if_changed(env, report)?;
        }
        Ok(())
    }

    fn online_lists(
        &mut self,
        keys: &BTreeSet<&'static str>,
        opts: &ListOptions,
        report: &mut ListsReport,
        progress: &mut impl FnMut(Step),
    ) -> Result<(), Error> {
        let at = now(&self.conn)?;
        let due = due(&self.conn, keys, opts.max_age_days, &at)?;
        if due.is_empty() || opts.cancel.is_set() {
            return Ok(());
        }
        let mut failed: BTreeSet<&'static str> = BTreeSet::new();
        let mut errors = Vec::new();
        self.download_systems_with(
            &due,
            false,
            &opts.cancel,
            |n, total| {
                progress(Step::new(
                    n as u64,
                    total as u64,
                    format!("lists: {}", due.join(", ")),
                ))
            },
            |key, source, r| {
                if let Err(e) = r {
                    failed.insert(key);
                    errors.push(format!("{key}: {source}: {e}"));
                }
            },
        )?;
        let done: Vec<&str> = due.iter().copied().filter(|k| !failed.contains(k)).collect();
        for key in &done {
            checked_now(&self.conn, &format!("list:checked:{key}"))?;
        }
        if !done.is_empty() {
            report
                .updated
                .push(format!("DATs and lists: {}", done.join(", ")));
        }
        report.problems.extend(errors);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_follows_the_age_and_skips_systems_without_sources() {
        let lib = Library::open_in_memory().unwrap();
        let keys: BTreeSet<&'static str> = ["SNES", "ScummVM"].into_iter().collect();
        let at = "2026-10-21T12:00:00Z";
        assert_eq!(
            due(&lib.conn, &keys, 7, at).unwrap(),
            ["SNES"],
            "ScummVM has no DAT source"
        );
        set_stamp(&lib.conn, "list:checked:SNES", "2026-10-18T12:00:00Z").unwrap();
        assert!(due(&lib.conn, &keys, 7, at).unwrap().is_empty(), "3 days old");
        assert_eq!(
            due(&lib.conn, &keys, 2, at).unwrap(),
            ["SNES"],
            "older than 2 days"
        );
        assert!(due(&lib.conn, &keys, 0, at).unwrap().is_empty(), "0: never");
    }
}
