use std::collections::{BTreeSet, HashMap};

use rusqlite::Connection;

use crate::Error;

use super::{GameRef, Member};

/// The one game all of a path's matched entries agree on, or why there
/// isn't one.
pub(super) fn common_game(matched: &[&Member], is_archive: bool) -> Result<GameRef, String> {
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

/// Longest name [`sanitize`] gives, in bytes: room left under the usual
/// 255 for an extension and romforge's temporary `.<name>.romforge-part`.
const MAX_NAME: usize = 200;

/// A name safe as one path component on Linux, macOS and Windows: no
/// separators or characters Windows refuses, no trailing dot or space, not
/// a name Windows reserves (`CON`, `NUL`, `COM1`... get a `_`), and at most
/// [`MAX_NAME`] bytes.
pub(crate) fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let mut s = s.trim().trim_end_matches('.').trim();
    if s.len() > MAX_NAME {
        let mut end = MAX_NAME;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s = s[..end].trim_end().trim_end_matches('.').trim_end();
    }
    if s.is_empty() || s == "." || s == ".." {
        return "_".into();
    }
    let stem = s.split('.').next().unwrap_or(s).trim_end().to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0');
    if reserved {
        match s.split_once('.') {
            Some((a, b)) => format!("{}_.{b}", a.trim_end()),
            None => format!("{s}_"),
        }
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
pub(crate) type CloneGroups = HashMap<(String, String), (String, i64)>;

pub(crate) fn clone_groups(conn: &Connection) -> Result<CloneGroups, Error> {
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
pub(crate) fn rank(name: &str, regions: &[String]) -> (bool, i64, Vec<u64>) {
    let n = crate::names::parse(name);
    let region = n
        .regions
        .iter()
        .filter_map(|r| regions.iter().position(|w| w.eq_ignore_ascii_case(r)))
        .min()
        .map_or(-1, |p| (regions.len() - p) as i64);
    (n.is_retail(), region, n.revision())
}
