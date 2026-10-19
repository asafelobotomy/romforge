use std::path::Path;

use rusqlite::{Connection, params};

use super::apply::patch_rom;
use super::found::stem_lower;
use super::pair::LibFile;
use super::plan::{Form, tag_of};
use super::read::read_patch;
use crate::cleanup;
use crate::patch;
use crate::view::{Derived, Relation};
use crate::{Error, systems};

/// Name the unknown files that a paired patch beside them (in the same
/// folder or zip) proves are that patch applied to your verified copy of
/// its game: "<Game> [Translation: <tag>]", romforge naming it, never
/// verified. Tried on files the patch could have made: by its target's size
/// and CRC32 (BPS / UPS), else of the same name as the patch. Cartridges
/// only. Returns how many files it named.
pub(super) fn name_patched(conn: &Connection) -> Result<usize, Error> {
    struct Paired {
        path: String,
        system_id: i64,
        title: String,
        relation: String,
        target: Option<(u64, String)>,
        name: String,
    }
    let patches: Vec<Paired> = conn
        .prepare(
            "SELECT path, system_id, title, relation, target_size, target_crc32, name FROM patch_files
             WHERE title IS NOT NULL",
        )?
        .query_map([], |r| {
            Ok(Paired {
                path: r.get(0)?,
                system_id: r.get(1)?,
                title: r.get(2)?,
                relation: r.get(3)?,
                target: match (r.get::<_, Option<u64>>(4)?, r.get::<_, Option<String>>(5)?) {
                    (Some(s), Some(c)) => Some((s, c)),
                    _ => None,
                },
                name: r.get(6)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    if patches.is_empty() {
        return Ok(0);
    }
    let unknown: Vec<LibFile> = conn
        .prepare(
            "SELECT path, member, status, size, crc32, sha1 FROM files f
             WHERE status = 'unknown'
               AND NOT EXISTS (SELECT 1 FROM manual_ids m WHERE m.sha1 = f.sha1 AND m.size = f.size)
               AND NOT EXISTS (SELECT 1 FROM proofs_declined x WHERE x.sha1 = f.sha1 AND x.size = f.size)",
        )?
        .query_map([], |r| {
            Ok(LibFile {
                path: r.get(0)?,
                member: r.get(1)?,
                status: r.get(2)?,
                size: r.get(3)?,
                crc32: r.get(4)?,
                sha1: r.get(5)?,
                stripped: None,
            })
        })?
        .collect::<Result<_, _>>()?;
    let mut named = 0;
    for p in &patches {
        let Some(system) = systems::by_id(p.system_id) else {
            continue;
        };
        if system.is_disc_based || system.arcade || system.folders != systems::Folders::No {
            continue;
        }
        let dir = Path::new(&p.path).parent();
        let stems = [stem_lower(&p.name), stem_lower(&p.path)];
        let candidates: Vec<&LibFile> = unknown
            .iter()
            .filter(|f| f.status == "unknown" && (Path::new(&f.path).parent() == dir || f.path == p.path))
            .filter(|f| match &p.target {
                Some((size, crc)) => f.size == *size && f.crc32 == *crc,
                None => {
                    let name = if f.member.is_empty() { &f.path } else { &f.member };
                    stems.contains(&stem_lower(name))
                }
            })
            .collect();
        if candidates.is_empty() {
            continue;
        }
        let Some((base_path, base_member)) = crate::suggest::verified_copy(conn, p.system_id, &p.title)?
        else {
            continue;
        };
        let Some(base) = patch::read_entry(Path::new(&base_path), &base_member) else {
            continue;
        };
        let Ok(pf) = read_patch(Path::new(&p.path)) else {
            continue;
        };
        let mut outs: Vec<String> = Vec::new();
        for form in [Form::Raw, Form::Dat] {
            if let Some(out) = patch_rom(&pf.data, &base, form) {
                outs.push(cleanup::sha1_of(&out[..])?);
            }
        }
        let relation = Relation::parse(&p.relation).unwrap_or(Relation::Hack);
        let described = patch::ppf_records(&pf.data)
            .map(|x| x.description)
            .filter(|d| !d.trim().is_empty());
        let tag = described.unwrap_or_else(|| tag_of(&pf.name));
        let derived = Derived {
            relation,
            base: p.title.clone(),
            tag: (!tag.is_empty()).then_some(tag),
        };
        for f in candidates {
            if !outs.contains(&f.sha1) {
                continue;
            }
            named += conn.execute(
                concat!(
                    "INSERT INTO manual_ids (sha1, size, system_id, title, created_at, base, relation, tag, proof)
                     VALUES (?1, ?2, ?3, ?4, ",
                    crate::db::now!(),
                    ", ?5, ?6, ?7, ?8) ON CONFLICT(sha1, size) DO NOTHING"
                ),
                params![
                    f.sha1,
                    f.size,
                    p.system_id,
                    derived.title(),
                    derived.base,
                    relation.as_str(),
                    derived.tag,
                    pf.name
                ],
            )?;
        }
    }
    Ok(named)
}

/// What made a patched file: the patch, and the game it was applied to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PatchedFrom {
    pub base: String,
    pub patch: String,
    pub kind: String,
    pub created_at: String,
}

/// For a file romforge patched: what made it.
pub(crate) fn made_by(conn: &Connection, sha1: &str, size: u64) -> Result<Option<PatchedFrom>, Error> {
    use rusqlite::OptionalExtension;
    Ok(conn
        .query_row(
            "SELECT base, patch_name, kind, created_at FROM patched WHERE sha1 = ?1 AND size = ?2",
            params![sha1, size],
            |r| {
                Ok(PatchedFrom {
                    base: r.get(0)?,
                    patch: r.get(1)?,
                    kind: r.get(2)?,
                    created_at: r.get(3)?,
                })
            },
        )
        .optional()?)
}
