use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use super::disc::write_disc;
use super::plan::{Form, PatchPlan, dat_form};
use super::read::{PatchFile, read_patch};
use crate::cleanup;
use crate::organize::sanitize;
use crate::patch;
use crate::{Error, layout, systems};

/// What [`apply`] made.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Patched {
    pub batch_id: i64,
    pub target: PathBuf,
    pub title: String,
    /// SHA1 and size of the patched ROM (or track).
    pub sha1: String,
    pub size: u64,
}

/// Apply a plan: write the new file, journal it (a cleanup batch, so
/// `cleanup undo` removes it), name it as a derivative of its game, and
/// remember the patch that made it.
pub fn apply(conn: &mut Connection, plan: &PatchPlan, staging_root: &Path) -> Result<Patched, Error> {
    let p = read_patch(&plan.patch)?;
    let system = systems::by_key(&plan.system).ok_or_else(|| Error::UnknownSystem(plan.system.clone()))?;
    let dir = plan.target.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    conn.execute(
        concat!(
            "INSERT INTO cleanup_batches (created_at, keep_originals) VALUES (",
            crate::db::now!(),
            ", 1)"
        ),
        [],
    )?;
    let batch = conn.last_insert_rowid();
    let staging = staging_root.join(format!("patch-{batch}"));
    let name = plan
        .target
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let part = dir.join(layout::part(&name));
    let _ = std::fs::remove_file(&part);
    let made = std::fs::create_dir_all(&staging)
        .map_err(Error::from)
        .and_then(|()| {
            if plan.disc {
                write_disc(plan, &p, &staging, &part)
            } else {
                write_cartridge(plan, &p, &staging, &part)
            }
        })
        // Never over a file that appeared meanwhile.
        .and_then(|made| cleanup::place(&part, &plan.target).map(|()| made));
    let _ = std::fs::remove_dir_all(&staging);
    let (sha1, size) = match made {
        Ok(x) => x,
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            conn.execute("DELETE FROM cleanup_batches WHERE batch_id = ?1", [batch])?;
            return Err(e);
        }
    };
    cleanup::journal_output(conn, batch, &plan.target, "file")?;
    let tx = conn.transaction()?;
    tx.execute(
        concat!(
            "INSERT INTO manual_ids (sha1, size, system_id, title, created_at, base, relation, tag)
             VALUES (?1, ?2, ?3, ?4, ",
            crate::db::now!(),
            ", ?5, ?6, ?7)
             ON CONFLICT(sha1, size) DO UPDATE SET system_id = excluded.system_id, title = excluded.title,
               base = excluded.base, relation = excluded.relation, tag = excluded.tag, proof = NULL"
        ),
        params![
            sha1,
            size,
            system.id,
            plan.title,
            plan.base,
            plan.relation.as_str(),
            (!plan.tag.is_empty()).then_some(&plan.tag)
        ],
    )?;
    tx.execute(
        concat!(
            "INSERT INTO patched (sha1, size, base, system_id, patch_name, patch_sha1, kind, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ",
            crate::db::now!(),
            ")
             ON CONFLICT(sha1, size) DO UPDATE SET base = excluded.base, patch_name = excluded.patch_name,
               patch_sha1 = excluded.patch_sha1, kind = excluded.kind"
        ),
        params![sha1, size, plan.base, system.id, p.name, p.sha1, p.kind.name()],
    )?;
    tx.commit()?;
    Ok(Patched {
        batch_id: batch,
        target: plan.target.clone(),
        title: plan.title.clone(),
        sha1,
        size,
    })
}

/// A cartridge: patched in memory, zipped with its details (and the
/// patch's readme) in `romforge/`.
fn write_cartridge(
    plan: &PatchPlan,
    p: &PatchFile,
    staging: &Path,
    out: &Path,
) -> Result<(String, u64), Error> {
    let rom = &plan.parts[plan.rom];
    let mut data = Vec::new();
    cleanup::open_part(rom, staging)?.read_to_end(&mut data)?;
    let patched = patch_rom(&p.data, &data, plan.form)
        .ok_or_else(|| Error::Cleanup(format!("{} doesn't apply to {}", p.name, plan.base)))?;
    if patched == data {
        return Err(Error::Cleanup(format!(
            "{} changes nothing in {}",
            p.name, plan.base
        )));
    }
    // One ROM is named after the new game; a game of several keeps the
    // DAT's names, the one patched among them.
    let member = if plan.parts.len() == 1 {
        let ext = Path::new(&rom.name)
            .extension()
            .map_or(String::new(), |e| format!(".{}", e.to_string_lossy()));
        format!("{}{ext}", sanitize(&plan.title))
    } else {
        rom.name.clone()
    };
    let mut extras = vec![(
        format!("{}{}", sanitize(&plan.title), layout::DETAILS_SUFFIX),
        details(plan, p).into_bytes(),
    )];
    if let Some((n, d)) = &p.readme {
        extras.push((n.clone(), d.clone()));
    }
    let mut zip = zip::ZipWriter::new(std::fs::File::create_new(out)?);
    for (i, part) in plan.parts.iter().enumerate() {
        if i == plan.rom {
            zip.start_file(&member, cleanup::zip_options(patched.len() as u64))?;
            zip.write_all(&patched)?;
        } else {
            zip.start_file(&part.name, cleanup::zip_options(part.size))?;
            std::io::copy(&mut cleanup::open_part(part, staging)?, &mut zip)?;
        }
    }
    cleanup::write_extras(&mut zip, &extras)?;
    zip.finish()?.sync_all()?;
    let sha1 = cleanup::sha1_of(&patched[..])?;
    let mut back = zip::ZipArchive::new(std::io::BufReader::new(std::fs::File::open(out)?))?;
    if cleanup::sha1_of(back.by_name(&member)?)? != sha1 {
        return Err(Error::Cleanup(format!(
            "{}: doesn't read back as written",
            out.display()
        )));
    }
    Ok((sha1, patched.len() as u64))
}

/// A cartridge ROM patched in `form`: in the DAT's form, a copier header
/// taken off goes back on (a byte order stays the DAT's).
pub(super) fn patch_rom(patch: &[u8], data: &[u8], form: Form) -> Option<Vec<u8>> {
    match (form, dat_form(data)) {
        (Form::Dat, Some((rule, body))) => {
            let new = patch::apply(patch, &body)?;
            Some(if rule.reorder == crate::headers::Reorder::None {
                let mut v = data[..rule.len].to_vec();
                v.extend_from_slice(&new);
                v
            } else {
                new
            })
        }
        _ => patch::apply(patch, data),
    }
}

/// The new game's details file.
fn details(plan: &PatchPlan, p: &PatchFile) -> String {
    let base_sha1: Option<String> = if plan.disc {
        plan.parts
            .iter()
            .find(|x| !x.name.to_lowercase().ends_with(".cue"))
            .map(|x| x.sha1.clone())
    } else {
        plan.parts.get(plan.rom).map(|x| x.sha1.clone())
    };
    format!(
        "# {}\n\n- Platform: {}\n- Status: {} of {}, patched by romforge (identified by you, not verified)\n\
         - Patch: {} ({}, SHA1 {})\n- Applied to: {}{}\n",
        plan.title,
        plan.system,
        plan.relation.as_str(),
        plan.base,
        p.name,
        p.kind.name(),
        p.sha1,
        plan.applies_to,
        base_sha1.map_or(String::new(), |s| format!(" (SHA1 {s})")),
    )
}
