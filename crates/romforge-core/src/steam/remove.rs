//! Taking romforge out of Steam again.

use std::path::{Path, PathBuf};

use super::vdf::{self, Map, Value};
use super::{
    ART, entries, grid, is_ours, load, ours, refuse_while_running, running, save, template_path, users,
};
use crate::Error;

/// Take romforge out of the Steam in `root` for `account`: its shortcut,
/// its art, and the template once no account has romforge any more.
/// Returns whether there was a shortcut (none: nothing is touched).
pub fn remove(root: &Path, account: &str) -> Result<bool, Error> {
    remove_unless(root, account, running())
}

pub(super) fn shortcuts_of(root: &Path, account: &str) -> PathBuf {
    root.join("userdata").join(account).join("config/shortcuts.vdf")
}

pub(super) fn remove_art(grid: &Path, id: u32) {
    for (suffix, _) in ART {
        let _ = std::fs::remove_file(grid.join(format!("{id}{suffix}")));
    }
}

pub(super) fn remove_unless(root: &Path, account: &str, busy: bool) -> Result<bool, Error> {
    refuse_while_running(busy)?;
    let path = shortcuts_of(root, account);
    let top = load(&path)?;
    let all = entries(&top);
    let Some(mine) = ours(&all, account)? else {
        return Ok(false);
    };
    let rest: Vec<Map> = all.into_iter().filter(|e| !is_ours(e)).collect();
    save(&path, top, rest)?;
    if let Some(Value::Int(id)) = vdf::get(&mine, "appid") {
        remove_art(&grid(root, account), *id);
    }
    // The template is Steam's, shared by every account here.
    let elsewhere = users(root)
        .iter()
        .filter(|u| u.account != account)
        .any(|u| load(&shortcuts_of(root, &u.account)).is_ok_and(|top| entries(&top).iter().any(is_ours)));
    if !elsewhere {
        let _ = std::fs::remove_file(template_path(root));
    }
    Ok(true)
}
