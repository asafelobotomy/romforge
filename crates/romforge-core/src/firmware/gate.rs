//! Milestone 20, step 5: before a core or emulator starts a game, whether
//! it has the BIOS files it can't run without (`launch` then tries the
//! next one, or says which file is missing). Nothing is held back without
//! a RomForge folder; without RetroBIOS's list or a profile for it, only an
//! emulator that can't run without a BIOS and has none in its folder.

use rusqlite::Connection;

use super::check::Checker;
use super::ready::lacking;
use super::{FILE, FileReq, data, journal, slots};
use crate::launch::{Env, KNOWN_EMULATORS};
use crate::systems::System;

/// The required files `name` (`retroarch:<core>`, or an emulator as the
/// launch table names it) lacks to start `system`'s games; empty: none.
pub fn missing(conn: &Connection, env: &Env, system: &System, name: &str) -> Vec<String> {
    let Some(home) = env.home.as_ref() else {
        return Vec::new();
    };
    let Some(profiles) = env
        .firmware_list
        .as_ref()
        .and_then(|dir| std::fs::read(dir.join(FILE)).ok())
        .and_then(|bytes| data::parse(&bytes).ok())
        .map(|(profiles, _)| profiles)
    else {
        return no_list(home, name);
    };
    let Some(slot) = slots(&profiles, home, env)
        .into_iter()
        .find(|s| s.system.key == system.key && s.name == name && s.profile.is_some())
    else {
        return no_list(home, name);
    };
    // Only the files that decide it are looked at (no big optional ones).
    let decides = |f: &&FileReq| f.required && !f.hle && slot.only_for(f).is_none();
    let reqs: Vec<&FileReq> = slot.files().filter(decides).collect();
    let mut check = Checker::with_picked(journal::picked(conn).unwrap_or_default());
    let files: Vec<_> = reqs.iter().map(|f| check.state(&slot, f)).collect();
    lacking(&reqs, &files)
}

/// Without RetroBIOS's list (or a profile in it): an emulator that can't
/// start a game without a BIOS lacks one when its folder in `Bios/` has
/// no file, rather than starting into its own "no BIOS" message (and that
/// time counted as play).
fn no_list(home: &crate::home::Home, name: &str) -> Vec<String> {
    let Some(e) = KNOWN_EMULATORS.iter().find(|e| e.name == name && e.needs_bios) else {
        return Vec::new();
    };
    let dir = home.bios().join(e.settings_name());
    let has_file = walkdir::WalkDir::new(&dir)
        .into_iter()
        .filter_map(Result::ok)
        .any(|f| f.file_type().is_file());
    if has_file {
        Vec::new()
    } else {
        vec![format!("BIOS file in {}", dir.display())]
    }
}
