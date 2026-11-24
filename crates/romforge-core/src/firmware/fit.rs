//! Which places a file only whose name fits is offered for. A name says
//! nothing about which emulator or which region a file is for: Cemu's
//! `keys.txt` and `otp.bin` are not Azahar's, and a GameCube `JAP/IPL.bin`
//! is not the `EUR` one. The dropped file's own folders often say, so:
//!
//! 1. If its folders name an emulator (`Cemu/`, `saves/Azahar/`), by a
//!    standalone emulator's folder in `Bios/` or a word of the emulator or
//!    core that loads a place (`RetroArch: … (QUASI88)`), only that
//!    emulator's places.
//! 2. If its folders name one of sibling folders a place sits in (`GC/JAP/`
//!    among `GC/EUR`, `GC/JAP`, `GC/USA`), not the other siblings.
//!
//! Where its folders say nothing, every place of its name is offered, as
//! before. Files matched by their contents never come here.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use super::walk::SourceId;

/// `hits` narrowed to the places `id`'s folders point to; `used_by` gives a
/// place's labels.
pub(super) fn narrow<'a, T>(
    id: &SourceId,
    roots: &[PathBuf],
    bios: &Path,
    hits: Vec<(&'a PathBuf, T)>,
    used_by: impl Fn(&T) -> &[String],
) -> Vec<(&'a PathBuf, T)> {
    let dirs = folders(id, roots);
    if dirs.is_empty() {
        return hits;
    }
    let named = |place: &Path, t: &T| {
        let rel = parts(place.strip_prefix(bios).unwrap_or(place));
        let own = rel.first().filter(|f| rel.len() > 1 && *f != "retroarch");
        own.is_some_and(|f| dirs.contains(f))
            || used_by(t).iter().any(|l| words(l).any(|w| dirs.contains(&w)))
    };
    let hits = if hits.iter().any(|(p, t)| named(p, t)) {
        hits.into_iter().filter(|(p, t)| named(p, t)).collect()
    } else {
        hits
    };
    // Sibling folders: a place whose folder the file's folders name rules
    // out the places in that folder's siblings.
    let folder = |p: &Path| {
        let up = p.parent()?;
        let name = up
            .strip_prefix(bios)
            .ok()
            .filter(|r| r.components().count() > 1)?;
        Some((up.parent()?.to_path_buf(), lower(name.file_name()?.to_str()?)))
    };
    let chosen: HashSet<PathBuf> = hits
        .iter()
        .filter_map(|(p, _)| folder(p))
        .filter(|(_, name)| dirs.contains(name))
        .map(|(above, _)| above)
        .collect();
    hits.into_iter()
        .filter(|(p, _)| match folder(p) {
            Some((above, name)) => !chosen.contains(&above) || dirs.contains(&name),
            None => true,
        })
        .collect()
}

/// The folders `id` sits in, from the folder the user dropped (its own name
/// included) down, and inside its archives; lower case.
fn folders(id: &SourceId, roots: &[PathBuf]) -> HashSet<String> {
    let mut out = HashSet::new();
    let from = roots
        .iter()
        .filter(|r| id.file.starts_with(r) && id.file != **r)
        .max_by_key(|r| r.components().count());
    if let Some(root) = from {
        let base = root.parent().unwrap_or(root);
        let rel = id.file.strip_prefix(base).unwrap_or(&id.file);
        out.extend(parts(rel.parent().unwrap_or(Path::new(""))));
    }
    for m in &id.members {
        let mut p: Vec<&str> = m.split('/').filter(|s| !s.is_empty()).collect();
        p.pop();
        out.extend(p.into_iter().map(lower));
    }
    out
}

fn parts(p: &Path) -> Vec<String> {
    p.components()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str().map(lower),
            _ => None,
        })
        .collect()
}

fn lower(s: &str) -> String {
    s.to_lowercase()
}

/// The words of the emulator a label names (a core's in its brackets:
/// `RetroArch: Nintendo - GameCube / Wii (Dolphin)` is Dolphin, not Wii) of
/// three letters or more, so that a folder named `ds` or `x` picks nothing.
fn words(label: &str) -> impl Iterator<Item = String> + '_ {
    let name = match label.strip_prefix("RetroArch:") {
        Some(rest) => rest.rsplit_once('(').map_or(rest, |(_, core)| core),
        None => label,
    };
    name.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
        .filter(|w| w.chars().count() >= 3)
        .map(lower)
}
