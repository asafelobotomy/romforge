//! The places emulators load BIOS files from, as [`super::import`] matches
//! dropped files against them: what each accepts, who loads it and for
//! which systems.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use super::{FileReq, Slot};
use crate::systems::SYSTEMS;

/// One place an emulator loads a file from: what it accepts there, who
/// loads it and for which systems (in the systems table's order).
pub(super) struct Place<'a> {
    pub(super) reqs: Vec<&'a FileReq>,
    pub(super) used_by: Vec<String>,
    pub(super) systems: Vec<&'static str>,
}

pub(super) fn places<'a>(slots: &'a [Slot<'a>]) -> BTreeMap<PathBuf, Place<'a>> {
    let mut out: BTreeMap<PathBuf, Place> = BTreeMap::new();
    for slot in slots {
        for f in slot.files() {
            let p = out.entry(slot.place(f)).or_insert(Place {
                reqs: Vec::new(),
                used_by: Vec::new(),
                systems: Vec::new(),
            });
            p.reqs.push(f);
            let label = slot.label();
            if !p.used_by.contains(&label) {
                p.used_by.push(label);
            }
            if !p.systems.contains(&slot.system.name) {
                p.systems.push(slot.system.name);
            }
        }
    }
    out
}

/// `names` (systems as the table names them) once each, in the table's
/// order.
pub(super) fn in_table_order<'a>(names: impl Iterator<Item = &'a &'static str>) -> Vec<&'static str> {
    let names: HashSet<&str> = names.copied().collect();
    SYSTEMS
        .iter()
        .map(|s| s.name)
        .filter(|n| names.contains(n))
        .fold(Vec::new(), |mut out, n| {
            if !out.contains(&n) {
                out.push(n);
            }
            out
        })
}

/// A zip, which is offered whole as well as by its members.
pub(super) fn is_zip(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".zip")
}

/// The archives emulators load as they are (a MAME machine's `fm7.zip`): a
/// dropped one of these names is also offered whole, not only its members.
pub(super) fn sets(places: &BTreeMap<PathBuf, Place>) -> HashSet<String> {
    places
        .values()
        .flat_map(|p| &p.reqs)
        .filter(|f| f.archive)
        .map(|f| f.name.to_ascii_lowercase())
        .collect()
}
