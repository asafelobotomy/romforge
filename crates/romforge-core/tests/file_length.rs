//! House rule 11: no code file over 400 lines, tests and the app included.
//! The systems table is data, not code, and is the one file let off.

use std::path::{Path, PathBuf};

const MAX_LINES: usize = 400;
const EXEMPT: &[&str] = &["crates/romforge-core/src/systems/table.rs"];
const ROOTS: &[&str] = &["crates", "app/src", "app/src-tauri/src", "app/test", "themes"];
/// Code files outside those folders.
const FILES: &[&str] = &["app/vite.config.js", "app/src-tauri/build.rs"];
const KINDS: &[&str] = &["rs", "svelte", "js", "mjs", "css"];

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name();
        if path.is_dir() {
            if name != "target" && name != "node_modules" {
                walk(&path, out);
            }
        } else if path
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| KINDS.contains(&x))
        {
            out.push(path);
        }
    }
}

#[test]
fn no_code_file_is_over_400_lines() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for root in ROOTS {
        walk(&repo.join(root), &mut files);
    }
    for file in FILES {
        let path = repo.join(file);
        assert!(path.is_file(), "{file} is still there");
        files.push(path);
    }
    assert!(files.len() > 50, "the walk found the code");
    let mut over: Vec<String> = files
        .iter()
        .filter_map(|f| {
            let rel = f
                .strip_prefix(&repo)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let n = std::fs::read_to_string(f).ok()?.lines().count();
            (n > MAX_LINES && !EXEMPT.contains(&rel.as_str())).then(|| format!("{rel}: {n}"))
        })
        .collect();
    over.sort();
    assert!(
        over.is_empty(),
        "over {MAX_LINES} lines (split into child modules or components):\n{}",
        over.join("\n")
    );
}
