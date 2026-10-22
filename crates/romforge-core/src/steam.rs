//! Stage 2, step 7: romforge in Steam, as one non-Steam shortcut (the way
//! in on a Steam Deck, and in Big Picture), started in the fullscreen mode,
//! with romforge's own art and a Steam Input template for the Deck. Games
//! are picked inside romforge, which starts the emulators as its children
//! (gamescope loses windows that are handed off).
//!
//! Formats as Steam writes them, checked in Steam ROM Manager's, BoilR's
//! and EmuDeck's code: `userdata/<account>/config/shortcuts.vdf` (binary
//! VDF, [`vdf`]), the shortcut's id `crc32(Exe + AppName) | 0x80000000`,
//! art in `config/grid/` by that id, templates in
//! `controller_base/templates/`. Steam rewrites `shortcuts.vdf` when it
//! closes, so nothing is written while it runs. Every write is planned
//! first ([`plan`], the dry run) and `shortcuts.vdf` is backed up.

use std::path::{Path, PathBuf};

use crate::Error;

pub mod find;
pub mod template;
pub mod vdf;

pub use find::{User, roots, running, users};
use vdf::{Map, Value};

/// The shortcut's name in Steam's library.
pub const APP_NAME: &str = "romforge";
/// Its tag, which finds it again (whatever its Exe has become).
const TAG: &str = "romforge";
/// The backup of `shortcuts.vdf` from before romforge last changed it.
pub const BACKUP: &str = "shortcuts.vdf.romforge-backup";

/// romforge's art for Steam: (grid file after the id, bytes).
const ART: &[(&str, &[u8])] = &[
    ("p.png", include_bytes!("../assets/steam/capsule.png")),
    (".png", include_bytes!("../assets/steam/wide.png")),
    ("_hero.png", include_bytes!("../assets/steam/hero.png")),
    ("_logo.png", include_bytes!("../assets/steam/logo.png")),
    ("_icon.png", include_bytes!("../assets/steam/icon.png")),
];

/// A shortcut's id, from its `Exe` (as stored, quotes and all) and name.
pub fn app_id(exe: &str, name: &str) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(exe.as_bytes());
    h.update(name.as_bytes());
    h.finalize() | 0x8000_0000
}

/// What [`apply`] would do.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SteamPlan {
    pub root: PathBuf,
    pub account: String,
    pub shortcuts: PathBuf,
    /// `Exe` as Steam keeps it (quoted).
    pub exe: String,
    pub launch_options: String,
    pub app_id: u32,
    /// A romforge shortcut is there already (it is replaced).
    pub replaces: bool,
    /// Other shortcuts, kept as they are.
    pub others: usize,
    /// Every file written.
    pub files: Vec<PathBuf>,
}

fn quoted(p: &Path) -> String {
    format!("\"{}\"", p.display())
}

fn grid(root: &Path, account: &str) -> PathBuf {
    root.join("userdata").join(account).join("config/grid")
}

fn template_path(root: &Path) -> PathBuf {
    root.join("controller_base/templates").join(template::FILE)
}

/// The shortcuts in `path` (none if the file isn't there yet).
fn load(path: &Path) -> Result<Map, Error> {
    match std::fs::read(path) {
        Ok(b) => vdf::read(&b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(vec![("shortcuts".into(), Value::Map(Vec::new()))])
        }
        Err(e) => Err(e.into()),
    }
}

fn entries(top: &Map) -> Vec<Map> {
    match vdf::get(top, "shortcuts") {
        Some(Value::Map(m)) => m
            .iter()
            .filter_map(|(_, v)| match v {
                Value::Map(e) => Some(e.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A shortcut romforge made: its tag, else its name and Exe.
fn is_ours(e: &Map, exe: Option<&str>) -> bool {
    let tagged = matches!(vdf::get(e, "tags"), Some(Value::Map(t)) if t.iter().any(|(_, v)| *v == Value::Str(TAG.into())));
    tagged
        || (vdf::get_str(e, "AppName") == Some(APP_NAME)
            && exe.is_none_or(|x| vdf::get_str(e, "Exe") == Some(x)))
}

/// The exe Steam should start: the AppImage when romforge is one (it is
/// mounted somewhere new on each run), else the app: this program, or the
/// `romforge-app` beside the CLI (packages and builds put both together).
pub fn this_exe() -> Result<PathBuf, Error> {
    if let Some(a) = std::env::var_os("APPIMAGE") {
        return Ok(PathBuf::from(a));
    }
    let me = std::env::current_exe()?;
    if me
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("romforge-cli"))
    {
        let app = me.with_file_name("romforge-app");
        if !app.is_file() {
            return Err(Error::Settings(format!(
                "romforge-app isn't next to {} (Steam starts the app, not the CLI)",
                me.display()
            )));
        }
        return Ok(app);
    }
    Ok(me)
}

/// What adding romforge to the Steam in `root` for `account` would write
/// (nothing is written).
pub fn plan(root: &Path, account: &str, exe: &Path) -> Result<SteamPlan, Error> {
    let shortcuts = root.join("userdata").join(account).join("config/shortcuts.vdf");
    if !root.join("userdata").join(account).is_dir() {
        return Err(Error::Settings(format!(
            "no Steam user {account} in {}",
            root.display()
        )));
    }
    let exe_q = quoted(exe);
    let id = app_id(&exe_q, APP_NAME);
    let all = entries(&load(&shortcuts)?);
    let replaces = all.iter().any(|e| is_ours(e, None));
    let g = grid(root, account);
    let mut files = vec![shortcuts.clone()];
    files.extend(ART.iter().map(|(suffix, _)| g.join(format!("{id}{suffix}"))));
    files.push(template_path(root));
    Ok(SteamPlan {
        root: root.to_path_buf(),
        account: account.to_owned(),
        others: all.iter().filter(|e| !is_ours(e, None)).count(),
        shortcuts,
        exe: exe_q,
        launch_options: "--fullscreen".into(),
        app_id: id,
        replaces,
        files,
    })
}

fn refuse_while_running(busy: bool) -> Result<(), Error> {
    if busy {
        return Err(Error::Settings(
            "Steam is running: close it first (it rewrites its list of shortcuts when it closes)".into(),
        ));
    }
    Ok(())
}

/// Rewrite `shortcuts.vdf` with `entries` (renumbered), backing it up.
fn save(path: &Path, top: Map, entries: Vec<Map>) -> Result<(), Error> {
    if path.is_file() {
        std::fs::copy(path, path.with_file_name(BACKUP))?;
    }
    let numbered: Map = entries
        .into_iter()
        .enumerate()
        .map(|(i, e)| (i.to_string(), Value::Map(e)))
        .collect();
    let mut top = top;
    match top.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case("shortcuts")) {
        Some((_, v)) => *v = Value::Map(numbered),
        None => top.push(("shortcuts".into(), Value::Map(numbered))),
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::atomic::write(path, &vdf::write(&top))?;
    Ok(())
}

/// Do `p`: romforge's shortcut added (or put in place of its old one),
/// every other shortcut kept as it was, the art and the template written.
pub fn apply(p: &SteamPlan) -> Result<(), Error> {
    apply_unless(p, running())
}

fn apply_unless(p: &SteamPlan, busy: bool) -> Result<(), Error> {
    refuse_while_running(busy)?;
    let exe_path = p.exe.trim_matches('"');
    let start_dir = Path::new(exe_path).parent().map(quoted).unwrap_or_default();
    let top = load(&p.shortcuts)?;
    let old = entries(&top);
    let g = grid(&p.root, &p.account);
    let icon = g.join(format!("{}_icon.png", p.app_id));
    let int = |k: &str, v: u32| (k.to_owned(), Value::Int(v));
    let s = |k: &str, v: &str| (k.to_owned(), Value::Str(v.to_owned()));
    let mine: Map = vec![
        int("appid", p.app_id),
        s("AppName", APP_NAME),
        s("Exe", &p.exe),
        s("StartDir", &start_dir),
        s("icon", &icon.display().to_string()),
        s("ShortcutPath", ""),
        s("LaunchOptions", &p.launch_options),
        int("IsHidden", 0),
        int("AllowDesktopConfig", 1),
        int("AllowOverlay", 1),
        int("openvr", 0),
        int("Devkit", 0),
        s("DevkitGameID", ""),
        int("DevkitOverrideAppID", 0),
        int("LastPlayTime", 0),
        (
            "tags".into(),
            Value::Map(vec![("0".into(), Value::Str(TAG.into()))]),
        ),
    ];
    // Ours goes where the old one was, else at the end.
    let mut out = Vec::new();
    let mut placed = false;
    for e in old {
        if is_ours(&e, None) {
            if !placed {
                out.push(mine.clone());
                placed = true;
            }
        } else {
            out.push(e);
        }
    }
    if !placed {
        out.push(mine);
    }
    std::fs::create_dir_all(&g)?;
    for (suffix, bytes) in ART {
        crate::atomic::write(&g.join(format!("{}{suffix}", p.app_id)), bytes)?;
    }
    let t = template_path(&p.root);
    if let Some(dir) = t.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::atomic::write(&t, template::text().as_bytes())?;
    save(&p.shortcuts, top, out)
}

/// Take romforge out of the Steam in `root` for `account`: its shortcut,
/// its art and the template. Returns whether there was a shortcut.
pub fn remove(root: &Path, account: &str) -> Result<bool, Error> {
    remove_unless(root, account, running())
}

fn remove_unless(root: &Path, account: &str, busy: bool) -> Result<bool, Error> {
    refuse_while_running(busy)?;
    let path = root.join("userdata").join(account).join("config/shortcuts.vdf");
    let top = load(&path)?;
    let all = entries(&top);
    let (ours, rest): (Vec<Map>, Vec<Map>) = all.into_iter().partition(|e| is_ours(e, None));
    for e in &ours {
        if let Some(Value::Int(id)) = vdf::get(e, "appid") {
            for (suffix, _) in ART {
                let _ = std::fs::remove_file(grid(root, account).join(format!("{id}{suffix}")));
            }
        }
    }
    let _ = std::fs::remove_file(template_path(root));
    if ours.is_empty() {
        return Ok(false);
    }
    save(&path, top, rest)?;
    Ok(true)
}

/// Steam on this computer, as Settings shows it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Status {
    pub root: Option<PathBuf>,
    pub users: Vec<User>,
    pub running: bool,
    /// Accounts with romforge in their library, and the Exe each starts.
    pub added: Vec<(String, String)>,
    /// What a shortcut would start now.
    pub exe: Option<PathBuf>,
}

pub fn status() -> Status {
    let root = roots().into_iter().next();
    let users = root.as_deref().map(users).unwrap_or_default();
    let added = root
        .as_deref()
        .map(|r| {
            users
                .iter()
                .filter_map(|u| {
                    let path = r.join("userdata").join(&u.account).join("config/shortcuts.vdf");
                    let all = entries(&load(&path).ok()?);
                    let e = all.into_iter().find(|e| is_ours(e, None))?;
                    Some((
                        u.account.clone(),
                        vdf::get_str(&e, "Exe").unwrap_or_default().to_owned(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    Status {
        root,
        users,
        running: running(),
        added,
        exe: this_exe().ok(),
    }
}

#[cfg(test)]
mod tests;
