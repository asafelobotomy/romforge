//! Making the index: `themes.json` from a `romforge-themes` checkout's
//! `themes/<id>/` folders, each checked as romforge checks any theme
//! (`romforge-cli themes index`, which the repository's CI runs too).

use std::path::Path;

use super::{Index, Remote, RemoteFile, sha256};
use crate::Error;
use crate::themes::{API, list};

/// The index for the themes in `repo/themes/`, or every reason one is
/// refused (all of them, so a pull request sees them at once).
pub fn build_index(repo: &Path) -> Result<Index, Error> {
    let dir = repo.join("themes");
    let mut themes = Vec::new();
    let mut problems = Vec::new();
    for e in list(Some(&dir)).into_iter().filter(|e| !e.builtin) {
        let (Some(m), None) = (&e.manifest, &e.problem) else {
            problems.push(format!("{}: {}", e.id, e.problem.unwrap_or_default()));
            continue;
        };
        if m.version.trim().is_empty() {
            problems.push(format!("{}: no version in theme.json", e.id));
            continue;
        }
        if m.licence.trim().is_empty() {
            problems.push(format!("{}: no licence in theme.json", e.id));
            continue;
        }
        let root = dir.join(&e.id);
        let mut files = Vec::new();
        for (path, _) in super::super::walk(&root) {
            let rel = path
                .strip_prefix(&root)
                .map_err(|_| Error::Settings("a file out of its theme".into()))?;
            let rel = rel.to_string_lossy().replace('\\', "/");
            if rel.starts_with('.') || rel.contains("/.") {
                continue;
            }
            let bytes = std::fs::read(&path)?;
            files.push(RemoteFile {
                size: bytes.len() as u64,
                sha256: sha256(&bytes),
                path: rel,
            });
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let preview = if files.iter().any(|f| f.path == "preview.png") {
            format!("themes/{}/preview.png", e.id)
        } else {
            String::new()
        };
        let r = Remote {
            id: e.id.clone(),
            name: m.name.clone(),
            version: m.version.clone(),
            api: m.api,
            author: m.author.clone(),
            licence: m.licence.clone(),
            licence_url: m.licence_url.clone(),
            description: m.description.clone(),
            preview,
            files,
        };
        match r.problem() {
            Some(p) => problems.push(format!("{}: {p}", e.id)),
            None => themes.push(r),
        }
    }
    if !problems.is_empty() {
        return Err(Error::Settings(problems.join("\n")));
    }
    Ok(Index { api: API, themes })
}

/// `themes.json` as written: stable (sorted, pretty), so a diff shows
/// what changed.
pub fn index_text(index: &Index) -> String {
    serde_json::to_string_pretty(index).unwrap_or_default() + "\n"
}
