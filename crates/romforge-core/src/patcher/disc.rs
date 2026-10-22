use std::io::Write;
use std::path::{Path, PathBuf};

use super::plan::PatchPlan;
use super::read::PatchFile;
use crate::Error;
use crate::cleanup::{self, Format, Item, Part};
use crate::patch::{self, Kind};

/// A track (or image) on disk to patch: a loose file as it is, else unpacked
/// into `staging`.
fn on_disk(part: &Part, staging: &Path) -> Result<PathBuf, Error> {
    if part.member.is_empty()
        && !part
            .path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("chd"))
    {
        return Ok(part.path.clone());
    }
    let to = staging.join(format!("{}.in", part.sha1));
    if !to.exists() {
        let mut w = std::fs::File::create(&to)?;
        std::io::copy(&mut cleanup::open_part(part, staging)?, &mut w)?;
    }
    Ok(to)
}

/// A disc: its tracks (unpacked from a CHD with a cue sheet of its own, or
/// loose as they are), the first one patched, written as a CHD.
pub(super) fn write_disc(
    plan: &PatchPlan,
    p: &PatchFile,
    staging: &Path,
    out: &Path,
) -> Result<(String, u64), Error> {
    let mut parts = plan.parts.clone();
    // Tracks in a CHD: unpacked, with a cue from its track list.
    if let Some(chd) = parts.iter().find(|x| is_chd(&x.path)).map(|x| x.path.clone()) {
        parts = unpack_chd(&chd, staging)?;
    }
    // The track the patch names first, then the others in order: one that
    // isn't the patch's source is refused by its checks.
    let order = track_order(&parts, p);
    let patched = staging.join("patched.bin");
    let mut first = None;
    let mut refused = None;
    for &i in &order {
        let source = on_disk(&parts[i], staging)?;
        let _ = std::fs::remove_file(&patched);
        match patch::apply_file(&p.data, &source, &patched) {
            Ok(()) => {
                first = Some(i);
                break;
            }
            Err(e) => {
                refused.get_or_insert(e);
                // Only formats with a check can say "not this track".
                if !matches!(p.kind, Kind::Xdelta | Kind::Ppf) {
                    break;
                }
            }
        }
    }
    let Some(first) = first else {
        return Err(refused.unwrap_or_else(|| Error::Cleanup(format!("{}: no track", plan.base))));
    };
    let sha1 = cleanup::sha1_of(std::io::BufReader::new(std::fs::File::open(&patched)?))?;
    let size = std::fs::metadata(&patched)?.len();
    if sha1 == parts[first].sha1 {
        return Err(Error::Cleanup(format!(
            "{} changes nothing in {}",
            p.name, plan.base
        )));
    }
    parts[first] = Part {
        name: parts[first].name.clone(),
        path: patched,
        member: String::new(),
        size,
        sha1: sha1.clone(),
    };
    let tracks: Vec<&Part> = parts
        .iter()
        .filter(|x| !x.name.to_lowercase().ends_with(".cue"))
        .collect();
    let format = if tracks.len() == 1 && tracks[0].name.to_lowercase().ends_with(".iso") {
        Format::ChdDvd
    } else {
        Format::ChdCd
    };
    let item = Item {
        row_id: plan.row_id.clone(),
        title: plan.title.clone(),
        system: plan.system.clone(),
        format,
        target: plan.target.clone(),
        sidecar: None,
        parts,
        trash: Vec::new(),
        kept: Vec::new(),
        copies: Vec::new(),
        sheet: None,
        existing: false,
        folder: None,
        code: None,
    };
    cleanup::write_chd(&item, staging, out)?;
    Ok((sha1, size))
}

pub(super) fn is_chd(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("chd"))
}

/// Whether a track file is the one a patch names: the same file name, or
/// the same "(Track N)".
pub(super) fn track_of(track: &str, named: &str) -> bool {
    let number = |s: &str| {
        let s = s.to_ascii_lowercase();
        let at = s.rfind("(track ")? + 7;
        s[at..].split(')').next()?.trim().parse::<u32>().ok()
    };
    track.eq_ignore_ascii_case(named) || number(track).is_some_and(|n| Some(n) == number(named))
}

/// A disc's tracks (indices into `parts`, cue sheets left out) in the order
/// to try a patch on: the one an xdelta names first, then the rest as they
/// come.
pub(super) fn track_order(parts: &[Part], p: &PatchFile) -> Vec<usize> {
    let mut order: Vec<usize> = (0..parts.len())
        .filter(|i| !parts[*i].name.to_lowercase().ends_with(".cue"))
        .collect();
    if p.kind == Kind::Xdelta
        && let Some((_, source)) = crate::vcdiff::names(&p.data)
        && let Some(at) = order.iter().position(|i| track_of(&parts[*i].name, &source))
    {
        let i = order.remove(at);
        order.insert(0, i);
    }
    order
}

/// A CHD's tracks written out as `.bin`s (or its `.iso`) in `staging`, with
/// a cue sheet made from its track list: every track's pregap is in its
/// `.bin`, so INDEX 01 comes after it.
fn unpack_chd(chd: &Path, staging: &Path) -> Result<Vec<Part>, Error> {
    let dir = staging.join("tracks");
    std::fs::create_dir_all(&dir)?;
    let mut out: Vec<Part> = Vec::new();
    let mut cue = String::new();
    let mut current = std::io::BufWriter::new(std::fs::File::create(dir.join("current"))?);
    let mut hasher = sha1::Sha1::default();
    let mut size = 0u64;
    crate::disc::read_chd(chd, &mut |o| {
        match o {
            crate::disc::ChdOut::Data(d) => {
                current.write_all(d)?;
                sha1::Digest::update(&mut hasher, d);
                size += d.len() as u64;
            }
            crate::disc::ChdOut::End(name, track) => {
                current.flush()?;
                let path = dir.join(&name);
                std::fs::rename(dir.join("current"), &path)?;
                current = std::io::BufWriter::new(std::fs::File::create(dir.join("current"))?);
                if let Some(t) = track {
                    let mode = match t.kind.as_str() {
                        "AUDIO" => "AUDIO",
                        "MODE1_RAW" => "MODE1/2352",
                        _ => "MODE2/2352",
                    };
                    cue += &format!("FILE \"{name}\" BINARY\n  TRACK {:02} {mode}\n", t.number);
                    if t.pregap > 0 {
                        let msf = |f: u64| format!("{:02}:{:02}:{:02}", f / 75 / 60, f / 75 % 60, f % 75);
                        cue += &format!("    INDEX 00 00:00:00\n    INDEX 01 {}\n", msf(t.pregap));
                    } else {
                        cue += "    INDEX 01 00:00:00\n";
                    }
                }
                out.push(Part {
                    name,
                    path,
                    member: String::new(),
                    size: std::mem::take(&mut size),
                    sha1: crate::hash::hex(&sha1::Digest::finalize(std::mem::take(&mut hasher))),
                });
            }
        }
        Ok(())
    })?;
    drop(current);
    let _ = std::fs::remove_file(dir.join("current"));
    if !cue.is_empty() {
        let stem = chd.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let path = dir.join(format!("{stem}.cue"));
        std::fs::write(&path, &cue)?;
        out.push(Part {
            name: format!("{stem}.cue"),
            sha1: cleanup::sha1_of(cue.as_bytes())?,
            size: cue.len() as u64,
            path,
            member: String::new(),
        });
    }
    Ok(out)
}
