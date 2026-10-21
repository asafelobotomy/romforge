//! Grouping a game's versions, and the category of each.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, params};

use super::patches::bare_row;
use super::{Category, GameRow, Relation, RowStatus};
use crate::Error;

/// Group the versions of each game: regions, revisions, translations and
/// hacks of one game share a group, keyed by its clone-list group, else
/// its DAT parent (`cloneof`), else its title without tags; translations
/// and hacks by their base game. Groups always include the system id
/// (`v:{system_id}:…`), so the same title on two platforms (or a remake
/// in the same year) stays two games in All games. The main row is the
/// best release in the Settings region order (a translation or hack only
/// when nothing else is there). Unidentified files are never grouped.
///
/// For a `listing`, patch rows join their game's group too, a game's
/// original is its main row (added as [`RowStatus::Missing`] when only
/// translations, hacks or patches of it are here), and every other version
/// gets its [`Category`] and the row it is made from. With Settings
/// `show_missing_versions`, DAT titles in the same group that you don't
/// have are listed as Missing too.
pub(super) fn group_versions(conn: &Connection, rows: &mut Vec<GameRow>, listing: bool) -> Result<(), Error> {
    use crate::organize::rank;
    let settings = crate::settings::load(conn)?;
    let regions = settings.region_order();
    let clones = crate::organize::clone_groups(conn)?;
    let bases: HashMap<(i64, String), (String, Option<Relation>)> = conn
        .prepare("SELECT DISTINCT system_id, title, base, relation FROM manual_ids WHERE base IS NOT NULL")?
        .query_map([], |r| {
            Ok((
                (r.get(0)?, r.get(1)?),
                (
                    r.get(2)?,
                    r.get::<_, Option<String>>(3)?
                        .as_deref()
                        .and_then(Relation::parse),
                ),
            ))
        })?
        .collect::<Result<_, _>>()?;
    let mut parent_of = conn.prepare(
        "SELECT g.cloneof FROM games g JOIN dats d ON d.dat_id = g.dat_id
         WHERE g.name = ?2 AND d.system_id = ?1 AND g.cloneof IS NOT NULL AND g.cloneof <> ''
         LIMIT 1",
    )?;
    // What a translation, hack or patch row is made from.
    let base_of = |r: &GameRow| -> Option<(String, Relation)> {
        if r.status == RowStatus::Patch {
            return r.patch_of.clone();
        }
        let (base, rel) = bases.get(&(r.system_id?, r.title.clone()))?;
        (r.status == RowStatus::Derivative).then(|| (base.clone(), rel.unwrap_or(Relation::Hack)))
    };
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut priority: HashMap<usize, i64> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        let identified = matches!(
            r.status,
            RowStatus::Verified | RowStatus::Incomplete | RowStatus::ByYou | RowStatus::Derivative
        ) || (listing && r.status == RowStatus::Patch);
        let (Some(sid), Some(key)) = (r.system_id, r.system.as_deref()) else {
            continue;
        };
        if !identified {
            continue;
        }
        let name = base_of(r).map(|(b, _)| b).unwrap_or_else(|| r.title.clone());
        let parent: Option<String> = parent_of.query_row(params![sid, name], |r| r.get(0)).optional()?;
        let (group, p) = group_key(&clones, sid, key, &name, parent.as_deref());
        if let Some(p) = p {
            priority.insert(i, p);
        }
        groups.entry(group).or_default().push(i);
    }
    let original = |r: &GameRow| {
        matches!(
            r.status,
            RowStatus::Verified | RowStatus::Incomplete | RowStatus::ByYou
        )
    };
    for idx in groups.values_mut() {
        // Only translations, hacks or patches: their original goes on top.
        if listing && !idx.iter().any(|&i| original(&rows[i])) {
            let first = &rows[idx[0]];
            let sid = first.system_id;
            let title = idx
                .iter()
                .filter_map(|&i| base_of(&rows[i]).map(|(b, _)| b))
                .max_by_key(|t| {
                    let (retail, region, revision) = rank(t, &regions);
                    (retail, region, revision, std::cmp::Reverse(t.clone()))
                })
                .unwrap_or_else(|| first.title.clone());
            let id = format!("x:{}:{title}", sid.map_or("-".to_owned(), |s| s.to_string()));
            rows.push(bare_row(id, sid, &title, RowStatus::Missing));
            idx.push(rows.len() - 1);
        }
    }
    if listing && settings.show_missing_versions {
        add_missing_from_dats(conn, rows, &mut groups, &clones)?;
    }
    for (group, idx) in groups {
        if idx.len() < 2 {
            continue;
        }
        let best = *idx
            .iter()
            .max_by_key(|&&i| {
                let r = &rows[i];
                let (retail, region, revision) = rank(&r.title, &regions);
                (
                    r.status == RowStatus::Missing,
                    r.status != RowStatus::Patch,
                    r.status != RowStatus::Derivative,
                    r.status != RowStatus::Incomplete,
                    retail,
                    region,
                    -priority.get(&i).copied().unwrap_or(1),
                    revision,
                    std::cmp::Reverse(r.title.clone()),
                )
            })
            .unwrap_or(&idx[0]);
        // Prefer an owned original over a Missing placeholder as main.
        let best = idx
            .iter()
            .copied()
            .filter(|&i| original(&rows[i]))
            .max_by_key(|&i| {
                let r = &rows[i];
                let (retail, region, revision) = rank(&r.title, &regions);
                (
                    retail,
                    region,
                    -priority.get(&i).copied().unwrap_or(1),
                    revision,
                    std::cmp::Reverse(r.title.clone()),
                )
            })
            .unwrap_or(best);
        let main_title = rows[best].name.title.clone();
        for &i in &idx {
            let category = (listing && i != best).then(|| match base_of(&rows[i]) {
                Some((_, rel)) => match (rows[i].status, rel) {
                    (RowStatus::Patch, Relation::Translation) => Category::PatchTranslation,
                    (RowStatus::Patch, Relation::Hack) => Category::PatchHack,
                    (_, Relation::Translation) => Category::Translation,
                    (_, Relation::Hack) => Category::Hack,
                },
                None if is_edition(&rows[i].title) => Category::Edition,
                None => Category::Region,
            });
            let r = &mut rows[i];
            r.group = Some(group.clone());
            r.main = i == best;
            r.versions = if i == best { idx.len() } else { 1 };
            r.category = category;
            r.edition = (category == Some(Category::Edition))
                .then(|| edition_label(&r.title, &main_title))
                .flatten();
        }
    }
    if listing {
        // The row each translation, hack and patch is made from.
        let owned: HashMap<(Option<i64>, String), String> = rows
            .iter()
            .filter(|r| matches!(r.status, RowStatus::Verified | RowStatus::ByYou))
            .map(|r| ((r.system_id, r.title.clone()), r.id.clone()))
            .collect();
        let ids: Vec<Option<String>> = rows
            .iter()
            .map(|r| base_of(r).and_then(|(b, _)| owned.get(&(r.system_id, b)).cloned()))
            .collect();
        for (r, id) in rows.iter_mut().zip(ids) {
            r.base_id = id;
        }
    }
    Ok(())
}

/// The version group of DAT game `name` (its `parent` if a clone) on
/// system `sid` (key `sys`): its clone-list group, else its base title.
/// Also its clone-list priority, if it has one.
fn group_key(
    clones: &crate::organize::CloneGroups,
    sid: i64,
    sys: &str,
    name: &str,
    parent: Option<&str>,
) -> (String, Option<i64>) {
    let base = crate::organize::base_title(parent.unwrap_or(name));
    match clones.get(&(sys.to_owned(), base.clone())) {
        Some((grp, p)) => (format!("v:{sid}:c:{}", grp.to_lowercase()), Some(*p)),
        None => (format!("v:{sid}:t:{base}"), None),
    }
}

/// DAT titles in an active group that you don't have, as Missing rows.
fn add_missing_from_dats(
    conn: &Connection,
    rows: &mut Vec<GameRow>,
    groups: &mut BTreeMap<String, Vec<usize>>,
    clones: &crate::organize::CloneGroups,
) -> Result<(), Error> {
    let active: BTreeSet<String> = groups.keys().cloned().collect();
    let systems: BTreeSet<i64> = active
        .iter()
        .filter_map(|g| g.strip_prefix("v:")?.split(':').next()?.parse().ok())
        .collect();
    if systems.is_empty() {
        return Ok(());
    }
    let have: HashSet<(Option<i64>, String)> = rows
        .iter()
        .filter(|r| {
            matches!(
                r.status,
                RowStatus::Verified | RowStatus::Incomplete | RowStatus::ByYou | RowStatus::Missing
            )
        })
        .map(|r| (r.system_id, r.title.clone()))
        .collect();
    let mut stmt = conn.prepare(
        "SELECT DISTINCT d.system_id, s.key, g.name, NULLIF(g.cloneof, '')
         FROM games g
         JOIN dats d ON d.dat_id = g.dat_id
         JOIN systems s ON s.system_id = d.system_id
         WHERE d.system_id = ?1",
    )?;
    for sid in systems {
        for row in stmt.query_map(params![sid], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })? {
            let (sid, key, name, cloneof) = row?;
            if have.contains(&(Some(sid), name.clone())) {
                continue;
            }
            // The same key the rows you have got, so a clone named apart
            // from its parent ("Rockman" of "Mega Man") joins that group.
            let (group, _) = group_key(clones, sid, &key, &name, cloneof.as_deref());
            if !active.contains(&group) {
                continue;
            }
            let id = format!("x:{sid}:{name}");
            rows.push(bare_row(id, Some(sid), &name, RowStatus::Missing));
            groups.entry(group).or_default().push(rows.len() - 1);
        }
    }
    Ok(())
}

/// Words that make a release a special edition or re-release.
const EDITION_WORDS: &[&str] = &[
    "edition",
    "collector",
    "limited",
    "deluxe",
    "virtual console",
    "switch online",
    "greatest hits",
    "platinum",
    "player's choice",
    "players choice",
    "the best",
    "classics",
    "reprint",
];

/// What tells edition `title` apart from its group's main title: the words
/// its clean title adds ("Resident Evil - Director's Cut" under "Resident
/// Evil": "Director's Cut"), else the tag that makes it an edition
/// ("(Greatest Hits)"). A non-retail status is shown on its own.
fn edition_label(title: &str, main_title: &str) -> Option<String> {
    let clean = crate::names::parse(title).title;
    if !clean.eq_ignore_ascii_case(main_title) {
        let extra = clean
            .get(..main_title.len())
            .filter(|head| !main_title.is_empty() && head.eq_ignore_ascii_case(main_title))
            .map(|_| {
                clean[main_title.len()..]
                    .trim_start_matches([' ', '-', ':', '~'])
                    .trim()
            });
        return Some(match extra {
            Some(e) if !e.is_empty() => e.to_owned(),
            _ => clean,
        });
    }
    title
        .split(['(', '['])
        .skip(1)
        .filter_map(|t| t.split_once([')', ']']).map(|(tag, _)| tag.trim()))
        .find(|tag| {
            let t = tag.to_lowercase();
            EDITION_WORDS.iter().any(|w| t.contains(w))
        })
        .map(str::to_owned)
}

/// A special edition, re-release, or not a retail release, by its name.
fn is_edition(title: &str) -> bool {
    let n = crate::names::parse(title);
    let t = title.to_lowercase();
    n.status.is_some() || EDITION_WORDS.iter().any(|w| t.contains(w))
}

#[cfg(test)]
mod tests {
    use super::edition_label;

    #[test]
    fn edition_label_names_what_sets_it_apart() {
        assert_eq!(
            edition_label("Resident Evil - Director's Cut (USA)", "Resident Evil").as_deref(),
            Some("Director's Cut")
        );
        assert_eq!(
            edition_label("Crash Bandicoot (USA) (Greatest Hits)", "Crash Bandicoot").as_deref(),
            Some("Greatest Hits")
        );
        assert_eq!(
            edition_label("Pokemon Yellow Special Edition (USA)", "Pokemon Red").as_deref(),
            Some("Pokemon Yellow Special Edition")
        );
        assert_eq!(edition_label("Quest (USA) (Beta)", "Quest"), None);
    }
}
