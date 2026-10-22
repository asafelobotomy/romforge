//! Where Steam is, who uses it, and whether it is running.

use std::path::{Path, PathBuf};

use super::vdf::{self, Value};

/// SteamID64 minus this is the account id `userdata/` is named by.
const ID64_BASE: u64 = 76_561_197_960_265_728;

/// Steam's folders, first the one to use: `ROMFORGE_STEAM_ROOT` (tests and
/// odd installs), else the native install (SteamOS too), its old link, and
/// Flatpak's. Only those that are there.
pub fn roots() -> Vec<PathBuf> {
    if let Some(r) = std::env::var_os("ROMFORGE_STEAM_ROOT") {
        return vec![PathBuf::from(r)];
    }
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = Vec::new();
    for r in [
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ] {
        let real = r.canonicalize().unwrap_or(r.clone());
        if r.join("userdata").is_dir() && !out.iter().any(|o| o.canonicalize().ok().as_ref() == Some(&real)) {
            out.push(r);
        }
    }
    out
}

/// A Steam user on this computer.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct User {
    /// The `userdata/<account>` folder's name.
    pub account: String,
    pub name: String,
    /// The one Steam last signed in (else the newest sign-in).
    pub recent: bool,
}

/// The users of the Steam in `root`: those `loginusers.vdf` names, then any
/// other `userdata/` folder; the most recent first.
pub fn users(root: &Path) -> Vec<User> {
    let mut out: Vec<(User, u64)> = Vec::new();
    let text = std::fs::read_to_string(root.join("config/loginusers.vdf")).unwrap_or_default();
    if let Ok(top) = vdf::read_text(&text)
        && let Some(Value::Map(users)) = vdf::get(&top, "users")
    {
        for (id64, v) in users {
            let (Ok(id), Value::Map(u)) = (id64.parse::<u64>(), v) else {
                continue;
            };
            let Some(account) = id.checked_sub(ID64_BASE) else {
                continue;
            };
            let recent = vdf::get_str(u, "MostRecent") == Some("1");
            let stamp = vdf::get_str(u, "Timestamp")
                .and_then(|t| t.parse().ok())
                .unwrap_or(0);
            out.push((
                User {
                    account: account.to_string(),
                    name: vdf::get_str(u, "PersonaName")
                        .or(vdf::get_str(u, "AccountName"))
                        .unwrap_or_default()
                        .to_owned(),
                    recent,
                },
                if recent { u64::MAX } else { stamp },
            ));
        }
    }
    for e in std::fs::read_dir(root.join("userdata"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
    {
        let n = e.file_name().to_string_lossy().into_owned();
        if n != "0" && n.bytes().all(|b| b.is_ascii_digit()) && !out.iter().any(|(u, _)| u.account == n) {
            out.push((
                User {
                    account: n.clone(),
                    name: n,
                    recent: false,
                },
                0,
            ));
        }
    }
    // Only accounts that have a userdata folder (one signed in here).
    out.retain(|(u, _)| root.join("userdata").join(&u.account).is_dir());
    out.sort_by_key(|u| std::cmp::Reverse(u.1));
    let mut users: Vec<User> = out.into_iter().map(|(u, _)| u).collect();
    if let Some(first) = users.first_mut() {
        first.recent = true;
    }
    for u in users.iter_mut().skip(1) {
        u.recent = false;
    }
    users
}

/// Whether Steam's client is running (it rewrites `shortcuts.vdf` when it
/// closes, so romforge writes only while it isn't). Read from /proc.
pub fn running() -> bool {
    running_in(Path::new("/proc"))
}

pub(super) fn running_in(proc: &Path) -> bool {
    std::fs::read_dir(proc)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
        })
        .any(|e| {
            std::fs::read_to_string(e.path().join("comm"))
                .is_ok_and(|c| matches!(c.trim(), "steam" | "steamwebhelper"))
        })
}
