//! Stage 2, step 5: community themes from the `romforge-themes`
//! repository. Its `themes.json` ([`Index`]) lists each theme with every
//! file's size and SHA-256; romforge fetches the files one by one (no
//! archive, nothing to unpack) beside the index, checks each, then the
//! whole theme with the same checks as a hand-made one ([`super::list`]),
//! and only then swaps it in ([`install`]). Nothing is downloaded without
//! being asked, each theme's licence is shown first (the app and the CLI
//! ask), and none is bundled (house rules 3 and 10).
//!
//! The index is `INDEX_URL` (raw.githubusercontent.com: free, no account),
//! or `ROMFORGE_THEMES_INDEX`: another URL, or a local `themes.json` or the
//! folder holding it (a theme author's checkout, tests).

use std::path::{Component, Path, PathBuf};

use super::{API, MAX_TOTAL, mime, slug};
use crate::Error;

mod build;
pub use build::*;
mod install;
pub use install::*;

/// The community index.
pub const INDEX_URL: &str =
    "https://raw.githubusercontent.com/asafelobotomy/romforge-themes/main/themes.json";

/// `themes.json`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Index {
    pub api: u32,
    pub themes: Vec<Remote>,
}

/// A theme the index lists.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Remote {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api: u32,
    #[serde(default)]
    pub author: String,
    pub licence: String,
    #[serde(default)]
    pub licence_url: String,
    #[serde(default)]
    pub description: String,
    /// A picture of it, beside the index (`themes/<id>/preview.png`).
    #[serde(default)]
    pub preview: String,
    pub files: Vec<RemoteFile>,
}

/// One of a theme's files, in `themes/<id>/` beside the index.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct RemoteFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

impl Remote {
    pub fn size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    /// Why romforge won't install it, if it won't.
    pub fn problem(&self) -> Option<String> {
        if self.api != API {
            return Some(format!("made for theme API {}, romforge has {API}", self.api));
        }
        if !slug(&self.id) {
            return Some(format!("id {:?} isn't a slug", self.id));
        }
        if self.licence.trim().is_empty() {
            return Some("no licence".into());
        }
        if !self.files.iter().any(|f| f.path == "theme.json")
            || !self.files.iter().any(|f| f.path == "theme.css")
        {
            return Some("no theme.json or theme.css".into());
        }
        if self.size() > MAX_TOTAL {
            return Some(format!(
                "{} MB, more than {} MB",
                self.size() >> 20,
                MAX_TOTAL >> 20
            ));
        }
        for f in &self.files {
            let plain = Path::new(&f.path)
                .components()
                .all(|c| matches!(c, Component::Normal(_)));
            let known = f.path == "theme.json" || mime(&f.path).is_some();
            if !plain || !known || f.path.starts_with('.') {
                return Some(format!("{}: not a file a theme may hold", f.path));
            }
            if f.sha256.len() != 64 || !f.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Some(format!("{}: no SHA-256", f.path));
            }
        }
        None
    }
}

/// Where the index and the files beside it are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Url(String),
    /// The folder `themes.json` is in.
    Folder(PathBuf),
}

impl Source {
    /// `ROMFORGE_THEMES_INDEX`, else [`INDEX_URL`].
    pub fn from_env() -> Source {
        match std::env::var("ROMFORGE_THEMES_INDEX") {
            Ok(v) if !v.trim().is_empty() => Source::parse(v.trim()),
            _ => Source::Url(INDEX_URL.into()),
        }
    }

    /// A URL to `themes.json`, a local `themes.json`, or its folder.
    pub fn parse(s: &str) -> Source {
        if s.starts_with("https://") || s.starts_with("http://") {
            return Source::Url(s.into());
        }
        let p = PathBuf::from(s);
        if p.is_dir() {
            Source::Folder(p)
        } else {
            Source::Folder(p.parent().map(Path::to_path_buf).unwrap_or_default())
        }
    }

    /// Shown in `.romforge-source.json` and the CLI.
    pub fn describe(&self) -> String {
        match self {
            Source::Url(u) => u.clone(),
            Source::Folder(p) => p.join("themes.json").display().to_string(),
        }
    }

    /// `rel` (a plain relative path) beside the index, at most `max` bytes.
    pub fn get(&self, rel: &str, max: u64) -> Result<Vec<u8>, Error> {
        if !Path::new(rel)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        {
            return Err(Error::Http(format!("{rel}: not a plain path")));
        }
        match self {
            Source::Url(u) => {
                let base = u.rsplit_once('/').map_or(u.as_str(), |(b, _)| b);
                let url = format!(
                    "{base}/{}",
                    rel.split('/')
                        .map(crate::fetch::encode_segment)
                        .collect::<Vec<_>>()
                        .join("/")
                );
                // ureq refuses a body that reaches its limit; the exact size
                // is checked against the index after.
                let req = crate::fetch::Request {
                    url: &url,
                    max_body: Some(max + 1),
                    ..crate::fetch::Request::get(&url)
                };
                crate::fetch::request(&req)?.ok_or_else(|| Error::Http(format!("{rel}: not found")))
            }
            Source::Folder(d) => {
                let path = d.join(rel);
                let len = std::fs::metadata(&path)?.len();
                if len > max {
                    return Err(Error::Http(format!("{rel}: {len} bytes, more than {max}")));
                }
                Ok(std::fs::read(path)?)
            }
        }
    }

    /// The index, its themes checked ([`Remote::problem`] still lists the
    /// refused ones, for the app to say why).
    pub fn index(&self) -> Result<Index, Error> {
        let bytes = self.get("themes.json", 4 << 20)?;
        let index: Index =
            serde_json::from_slice(&bytes).map_err(|e| Error::Http(format!("themes.json: {e}")))?;
        if index.api != API {
            return Err(Error::Http(format!(
                "the theme index is for theme API {}, romforge has {API}: update romforge",
                index.api
            )));
        }
        Ok(index)
    }
}

/// `r`'s preview picture, fetched once into `cache` (by id and version);
/// none when it has none.
pub fn preview(source: &Source, r: &Remote, cache: &Path) -> Result<Option<PathBuf>, Error> {
    if r.preview.is_empty() || mime(&r.preview) != Some("image/png") || !slug(&r.id) {
        return Ok(None);
    }
    let version: String = r
        .version
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.')
        .collect();
    let path = cache.join(format!("{}-{version}.png", r.id));
    if !path.is_file() {
        let bytes = source.get(&r.preview, 4 << 20)?;
        std::fs::create_dir_all(cache)?;
        crate::atomic::write(&path, &bytes)?;
    }
    Ok(Some(path))
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// `a` newer than `b`, by dotted numbers ("1.10.0" > "1.9.2"); anything
/// that isn't a number compares as text.
pub fn newer(a: &str, b: &str) -> bool {
    let parts = |s: &str| {
        s.split(['.', '-'])
            .map(|p| p.parse::<u64>().map_err(|_| p.to_owned()))
            .collect::<Vec<_>>()
    };
    let (pa, pb) = (parts(a), parts(b));
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).cloned().unwrap_or(Ok(0));
        let y = pb.get(i).cloned().unwrap_or(Ok(0));
        let ord = match (x, y) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            (Ok(_), Err(_)) => std::cmp::Ordering::Greater,
            (Err(_), Ok(_)) => std::cmp::Ordering::Less,
            (Err(x), Err(y)) => x.cmp(&y),
        };
        if ord != std::cmp::Ordering::Equal {
            return ord == std::cmp::Ordering::Greater;
        }
    }
    false
}

#[cfg(test)]
mod tests;
