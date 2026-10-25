//! Whether an emulator can start games with the files there are, and which
//! files decide it.

use std::collections::HashMap;

use super::{FileReq, FileState, State};

/// A cartridge coprocessor's ROM, loaded only for the games with that chip:
/// named `<chip>.program.rom` / `<chip>.data.rom` (bsnes and its kin).
pub(super) fn is_chip(name: &str) -> bool {
    name.ends_with(".program.rom") || name.ends_with(".data.rom")
}

/// RetroBIOS names a libretro core with or without its `_libretro`.
pub(super) fn same_core(listed: &str, core: &str) -> bool {
    listed == core || listed.strip_suffix("_libretro") == Some(core)
}

/// Whether an emulator can start games with `files` (checked, in the order
/// of `reqs`), and for which regions when its required files are regional.
pub(super) fn readiness(reqs: &[&FileReq], files: &[FileState]) -> (bool, Vec<String>) {
    // A zip with some members, or a file you placed, is there: which members
    // a game needs varies, and yours is your call.
    let ok = |f: &FileState| {
        matches!(
            f.state,
            State::Accepted | State::Present | State::Incomplete | State::PlacedByYou
        )
    };
    let pairs: Vec<(&FileReq, &FileState)> = reqs
        .iter()
        .copied()
        .zip(files)
        .filter(|(r, f)| r.required && !r.hle && f.only_for.is_none())
        .collect();
    // A group of alternatives needs one; so does a set of regional files.
    let mut groups: HashMap<&str, bool> = HashMap::new();
    let mut regions = Vec::new();
    let mut regional = false;
    let mut plain = true;
    for (r, f) in &pairs {
        if let Some(g) = &r.variant_group {
            *groups.entry(g).or_default() |= ok(f);
        } else if !r.regions.is_empty() {
            regional = true;
            if ok(f) {
                regions.extend(r.regions.iter().cloned());
            }
        } else {
            plain &= ok(f);
        }
    }
    regions.sort();
    regions.dedup();
    let ready = plain && groups.values().all(|g| *g) && (!regional || !regions.is_empty());
    (ready, regions)
}

/// What keeps an emulator from starting games, in the terms of
/// [`readiness`]: each required file not there, one of a group of
/// alternatives ("a or b"), one region's file where they are regional.
pub(super) fn lacking(reqs: &[&FileReq], files: &[FileState]) -> Vec<String> {
    let ok = |f: &FileState| {
        matches!(
            f.state,
            State::Accepted | State::Present | State::Incomplete | State::PlacedByYou
        )
    };
    let mut out = Vec::new();
    let mut groups: Vec<(&str, bool, Vec<&str>)> = Vec::new();
    let (mut regional, mut any_region) = (Vec::new(), false);
    for (r, f) in reqs.iter().zip(files) {
        if !r.required || r.hle || f.only_for.is_some() {
            continue;
        }
        if let Some(g) = &r.variant_group {
            match groups.iter_mut().find(|(n, ..)| n == g) {
                Some(e) => {
                    e.1 |= ok(f);
                    e.2.push(&r.name);
                }
                None => groups.push((g, ok(f), vec![&r.name])),
            }
        } else if !r.regions.is_empty() {
            any_region |= ok(f);
            regional.push(r.name.as_str());
        } else if !ok(f) {
            out.push(r.name.clone());
        }
    }
    for (_, have, names) in groups {
        if !have {
            out.push(names.join(" or "));
        }
    }
    if !regional.is_empty() && !any_region {
        out.push(format!("one region's {}", regional.join(", ")));
    }
    out
}
