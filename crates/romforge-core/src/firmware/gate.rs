//! Milestone 20, step 5: before a core or emulator starts a game, whether
//! it has the BIOS files it can't run without (`launch` then tries the
//! next one, or says which file is missing). Nothing is known, so nothing
//! is held back, without a RomForge folder, RetroBIOS's list or a profile
//! for it.

use rusqlite::Connection;

use super::check::Checker;
use super::ready::lacking;
use super::{FILE, FileReq, data, journal, slots};
use crate::launch::Env;
use crate::systems::System;

/// The required files `name` (`retroarch:<core>`, or an emulator as the
/// launch table names it) lacks to start `system`'s games; empty: none.
pub fn missing(conn: &Connection, env: &Env, system: &System, name: &str) -> Vec<String> {
    let (Some(home), Some(dir)) = (env.home.as_ref(), env.firmware_list.as_ref()) else {
        return Vec::new();
    };
    let Ok(bytes) = std::fs::read(dir.join(FILE)) else {
        return Vec::new();
    };
    let Ok((profiles, _)) = data::parse(&bytes) else {
        return Vec::new();
    };
    let Some(slot) = slots(&profiles, home, env)
        .into_iter()
        .find(|s| s.system.key == system.key && s.name == name && s.profile.is_some())
    else {
        return Vec::new();
    };
    // Only the files that decide it are looked at (no big optional ones).
    let decides = |f: &&FileReq| f.required && !f.hle && slot.only_for(f).is_none();
    let reqs: Vec<&FileReq> = slot.files().filter(decides).collect();
    let mut check = Checker::with_picked(journal::picked(conn).unwrap_or_default());
    let files: Vec<_> = reqs.iter().map(|f| check.state(&slot, f)).collect();
    lacking(&reqs, &files)
}
