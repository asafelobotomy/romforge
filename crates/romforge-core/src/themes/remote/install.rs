//! Installing, updating and removing a community theme, and what the
//! index's themes are here.

use std::path::Path;

use super::{Remote, Source, newer, sha256};
use crate::Error;

/// The file a downloaded theme keeps in its folder: where it came from.
pub const SOURCE_FILE: &str = ".romforge-source.json";

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Installed {
    pub index: String,
    pub version: String,
}

/// What the index's theme is here.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Status {
    NotInstalled,
    Installed {
        version: String,
    },
    UpdateAvailable {
        installed: String,
    },
    /// A folder of the user's own by that name, not from the index: left
    /// alone.
    Taken,
    /// The index's entry is refused (why).
    Refused {
        problem: String,
    },
}

/// Where a downloaded theme came from (none for a hand-made one).
pub fn installed(dir: &Path, id: &str) -> Option<Installed> {
    let text = std::fs::read_to_string(dir.join(id).join(SOURCE_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn status(dir: Option<&Path>, r: &Remote) -> Status {
    if let Some(problem) = r.problem() {
        return Status::Refused { problem };
    }
    if super::super::BUILTIN.iter().any(|(b, ..)| *b == r.id) {
        return Status::Taken;
    }
    let Some(dir) = dir else {
        return Status::NotInstalled;
    };
    if !dir.join(&r.id).exists() {
        return Status::NotInstalled;
    }
    match installed(dir, &r.id) {
        None => Status::Taken,
        Some(i) if newer(&r.version, &i.version) => Status::UpdateAvailable { installed: i.version },
        Some(i) => Status::Installed { version: i.version },
    }
}

/// Fetch `r`'s files from `source` into `dir/<id>/`: each checked by size
/// and SHA-256 in a folder of its own, then the whole theme checked as any
/// theme is, then swapped in for the one there (an older download). A
/// hand-made theme of that name is never touched. `progress` gets (bytes
/// so far, total). On any failure the theme there stays as it was.
pub fn install(
    dir: &Path,
    source: &Source,
    r: &Remote,
    mut progress: impl FnMut(u64, u64),
) -> Result<(), Error> {
    let refuse = |m: String| Err(Error::Settings(format!("{}: {m}", r.name)));
    if let Some(p) = r.problem() {
        return refuse(p);
    }
    match status(Some(dir), r) {
        Status::Taken => return refuse(format!("a theme of yours is already called {:?}", r.id)),
        Status::Refused { problem } => return refuse(problem),
        _ => {}
    }
    std::fs::create_dir_all(dir)?;
    let staging = dir.join(format!(".download-{}", r.id));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    let fetched = (|| -> Result<(), Error> {
        let (total, mut done) = (r.size(), 0u64);
        for f in &r.files {
            let bytes = source.get(&format!("themes/{}/{}", r.id, f.path), f.size)?;
            if bytes.len() as u64 != f.size || sha256(&bytes) != f.sha256.to_ascii_lowercase() {
                return Err(Error::Settings(format!(
                    "{}: {} isn't the file the index lists",
                    r.name, f.path
                )));
            }
            let to = staging.join(&f.path);
            if let Some(p) = to.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::write(&to, &bytes)?;
            done += f.size;
            progress(done, total);
        }
        // The same checks as any theme, under its own name.
        let check = dir.join(format!(".check-{}", r.id));
        if check.exists() {
            std::fs::remove_dir_all(&check)?;
        }
        std::fs::create_dir_all(&check)?;
        std::fs::rename(&staging, check.join(&r.id))?;
        let verdict = super::super::list(Some(&check))
            .into_iter()
            .find(|e| e.id == r.id && !e.builtin);
        std::fs::rename(check.join(&r.id), &staging)?;
        std::fs::remove_dir_all(&check)?;
        match verdict {
            Some(e) if e.usable() => Ok(()),
            Some(e) => Err(Error::Settings(format!(
                "{}: {}",
                r.name,
                e.problem.unwrap_or_default()
            ))),
            None => Err(Error::Settings(format!("{}: not a theme", r.name))),
        }
    })();
    if let Err(e) = fetched {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    let mark = Installed {
        index: source.describe(),
        version: r.version.clone(),
    };
    let text = serde_json::to_string_pretty(&mark).map_err(|e| Error::Settings(e.to_string()))?;
    std::fs::write(staging.join(SOURCE_FILE), text)?;
    // Swap: the old download aside, the new one in, the old one gone.
    let target = dir.join(&r.id);
    let old = dir.join(format!(".old-{}", r.id));
    if old.exists() {
        std::fs::remove_dir_all(&old)?;
    }
    if target.exists() {
        std::fs::rename(&target, &old)?;
    }
    if let Err(e) = std::fs::rename(&staging, &target) {
        if old.exists() {
            let _ = std::fs::rename(&old, &target);
        }
        return Err(e.into());
    }
    if old.exists() {
        std::fs::remove_dir_all(&old)?;
    }
    Ok(())
}

/// Remove a downloaded theme. A hand-made one (no [`SOURCE_FILE`]) is
/// never removed: it is yours.
pub fn remove(dir: &Path, id: &str) -> Result<(), Error> {
    if !super::super::slug(id) {
        return Err(Error::Settings(format!("{id:?} isn't a theme's name")));
    }
    if installed(dir, id).is_none() {
        return Err(Error::Settings(format!(
            "{id} wasn't downloaded by romforge: remove its folder yourself"
        )));
    }
    std::fs::remove_dir_all(dir.join(id))?;
    Ok(())
}
