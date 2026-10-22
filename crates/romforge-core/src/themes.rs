//! Stage 2, step 4: the fullscreen mode's themes. A theme is data, never
//! code: `theme.json` ([`Manifest`]: layouts, colour schemes, options), one
//! `theme.css`, and its pictures and fonts. The page loads the CSS through
//! the app's `theme://` protocol ([`file`]), wrapped in
//! `@scope ([data-fs-root])` so it reaches the fullscreen mode only, under a
//! CSP that lets it load nothing from outside romforge.
//!
//! Theme API 1: the `data-fs-*` attributes and `--fs-*` variables
//! GUIDE.md lists; never the app's class names.
//!
//! romforge's own themes are in the repository's `themes/` folder, built
//! in ([`BUILTIN`]); the user's are folders in `RomForge/Themes/` (or
//! `themes/` beside the library's database without one).

use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

mod manifest;
pub use manifest::*;
mod choice;
pub use choice::*;
pub mod remote;

/// The theme every fallback lands on.
pub const DEFAULT: &str = "snes-uk";

/// Most CSS a theme may have, and most bytes in all.
pub const MAX_CSS: u64 = 256 * 1024;
pub const MAX_TOTAL: u64 = 8 * 1024 * 1024;

macro_rules! builtin {
    ($id:literal) => {
        (
            $id,
            include_str!(concat!("../../../themes/", $id, "/theme.json")),
            include_str!(concat!("../../../themes/", $id, "/theme.css")),
        )
    };
}

/// romforge's own themes: (folder, theme.json, theme.css).
pub const BUILTIN: &[(&str, &str, &str)] = &[
    builtin!("snes-uk"),
    builtin!("famicom"),
    builtin!("mega-drive"),
    builtin!("game-boy"),
];

/// A theme found, usable or refused.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Entry {
    /// Its folder's name (the manifest's id when usable).
    pub id: String,
    pub builtin: bool,
    /// Downloaded from the community index (else made by the user).
    pub downloaded: bool,
    pub manifest: Option<Manifest>,
    /// Why it is refused.
    pub problem: Option<String>,
}

impl Entry {
    pub fn usable(&self) -> bool {
        self.problem.is_none() && self.manifest.is_some()
    }
}

/// The built-in themes, then the user's in `dir` (by name), each checked.
pub fn list(dir: Option<&Path>) -> Vec<Entry> {
    let mut out: Vec<Entry> = BUILTIN
        .iter()
        .map(|(id, json, css)| entry(id, true, Ok((json.to_string(), css.to_string(), 0))))
        .collect();
    let Some(dir) = dir else { return out };
    let mut users: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .collect();
    users.sort();
    for id in users {
        let mut e = entry(&id, false, read(&dir.join(&id)));
        e.downloaded = dir.join(&id).join(remote::SOURCE_FILE).is_file();
        if BUILTIN.iter().any(|(b, ..)| *b == id) {
            e.problem = Some(format!(
                "{id:?} is a built-in theme's name: rename the folder and its id"
            ));
        }
        out.push(e);
    }
    out
}

/// The usable theme `id`, if there is one.
pub fn find(dir: Option<&Path>, id: &str) -> Option<Manifest> {
    list(dir)
        .into_iter()
        .find(|e| e.id == id && e.usable())
        .and_then(|e| e.manifest)
}

fn entry(id: &str, builtin: bool, files: Result<(String, String, u64), String>) -> Entry {
    let checked = files.and_then(|(json, css, size)| {
        if size > MAX_TOTAL {
            return Err(format!("{} MB in all; at most {}", size >> 20, MAX_TOTAL >> 20));
        }
        if css.len() as u64 > MAX_CSS {
            return Err(format!("theme.css is over {} KB", MAX_CSS >> 10));
        }
        let m = parse(&json, id)?;
        check_css(&css)?;
        Ok(m)
    });
    match checked {
        Ok(m) => Entry {
            id: id.to_owned(),
            builtin,
            downloaded: false,
            manifest: Some(m),
            problem: None,
        },
        Err(problem) => Entry {
            id: id.to_owned(),
            builtin,
            downloaded: false,
            manifest: None,
            problem: Some(problem),
        },
    }
}

/// A user theme's manifest, CSS and size in all.
fn read(dir: &Path) -> Result<(String, String, u64), String> {
    let text = |name: &str| std::fs::read_to_string(dir.join(name)).map_err(|e| format!("{name}: {e}"));
    let (json, css) = (text("theme.json")?, text("theme.css")?);
    let size = walk(dir).iter().map(|(_, m)| m.len()).sum();
    Ok((json, css, size))
}

/// Every file under `dir`, not following links.
fn walk(dir: &Path) -> Vec<(PathBuf, std::fs::Metadata)> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().filter_map(Result::ok) {
            let Ok(meta) = std::fs::symlink_metadata(e.path()) else {
                continue;
            };
            if meta.is_dir() {
                todo.push(e.path());
            } else if meta.is_file() {
                out.push((e.path(), meta));
            }
        }
    }
    out
}

/// A file of theme `id` to serve, with its type: `theme.css` checked and
/// wrapped in its scope, pictures and fonts as they are. `None` for
/// anything else: another type, a path out of the theme (`..`, a link), a
/// refused theme.
pub fn file(dir: Option<&Path>, id: &str, path: &str) -> Option<(Vec<u8>, &'static str)> {
    let mime = mime(path)?;
    if !slug(id) {
        return None;
    }
    if path == "theme.css" {
        let entry = list(dir).into_iter().find(|e| e.id == id)?;
        if !entry.usable() {
            return None;
        }
        let css = match BUILTIN.iter().find(|(b, ..)| *b == id) {
            Some((.., css)) if entry.builtin => css.to_string(),
            _ => std::fs::read_to_string(dir?.join(id).join("theme.css")).ok()?,
        };
        return Some((scoped(&css).into_bytes(), mime));
    }
    // Pictures and fonts: user themes only (the built-in ones have none).
    let root = dir?.join(id);
    let rel = Path::new(path);
    if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    let full = root.join(rel);
    let real = full.canonicalize().ok()?;
    if !real.starts_with(root.canonicalize().ok()?) || std::fs::symlink_metadata(&full).ok()?.is_symlink() {
        return None;
    }
    find(dir, id)?;
    Some((std::fs::read(real).ok()?, mime))
}

/// `css` as served: only inside the fullscreen mode.
pub fn scoped(css: &str) -> String {
    format!("@scope ([data-fs-root]) {{\n{css}\n}}\n")
}

/// The types a theme may hold, by extension.
fn mime(path: &str) -> Option<&'static str> {
    let ext = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "css" => "text/css",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        _ => return None,
    })
}

/// When theme `id` last changed (milliseconds; 0 for a built-in one), so
/// the page reloads it as it is edited.
pub fn stamp(dir: Option<&Path>, id: &str) -> u64 {
    if !slug(id) || BUILTIN.iter().any(|(b, ..)| *b == id) {
        return 0;
    }
    let Some(dir) = dir else { return 0 };
    walk(&dir.join(id))
        .iter()
        .filter_map(|(_, m)| m.modified().ok())
        .filter_map(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
