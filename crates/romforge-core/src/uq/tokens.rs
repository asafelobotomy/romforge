//! uQ's theme for the app's window: `tokens.json`, which uQ writes to
//! `$XDG_STATE_HOME/uq/theme/` (`~/.local/state/uq/theme/`). The app only
//! reads it; the vendored `uq-tokens.js` checks every value before it
//! becomes CSS. uQ swaps the whole `theme/` folder at once, so a watch
//! goes on its parent, [`dir`].

use std::path::{Path, PathBuf};

/// Bigger files are not read.
pub const MAX_BYTES: u64 = 64 * 1024;

/// uQ's state folder (`$XDG_STATE_HOME/uq`, else `~/.local/state/uq`),
/// whether or not it exists.
pub fn dir() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .map(|state| state.join("uq"))
}

/// Whether a change at `path`, seen by a watch on [`dir`], may be the
/// tokens'.
pub fn touches_tokens(dir: &Path, path: &Path) -> bool {
    path.starts_with(dir.join("theme"))
}

/// `tokens.json`'s text in `dir`, or none without uQ, for a file that
/// can't be read, isn't UTF-8 or is over [`MAX_BYTES`].
pub fn read_in(dir: &Path) -> Option<String> {
    let path = dir.join("theme/tokens.json");
    let meta = std::fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return None;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    (text.len() as u64 <= MAX_BYTES).then_some(text)
}

/// [`read_in`] uQ's own folder.
pub fn read() -> Option<String> {
    read_in(&dir()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_a_small_tokens_file() {
        let tmp = tempfile::tempdir().unwrap();
        let uq = tmp.path().join("uq");
        assert_eq!(read_in(&uq), None, "no uQ");
        std::fs::create_dir_all(uq.join("theme")).unwrap();
        assert_eq!(read_in(&uq), None, "no file yet");
        std::fs::write(uq.join("theme/tokens.json"), r#"{"variant":"dark"}"#).unwrap();
        assert_eq!(read_in(&uq).as_deref(), Some(r#"{"variant":"dark"}"#));
        std::fs::write(uq.join("theme/tokens.json"), "x".repeat(MAX_BYTES as usize + 1)).unwrap();
        assert_eq!(read_in(&uq), None, "too big");
        std::fs::remove_file(uq.join("theme/tokens.json")).unwrap();
        std::fs::create_dir(uq.join("theme/tokens.json")).unwrap();
        assert_eq!(read_in(&uq), None, "not a file");
    }

    #[test]
    fn only_the_theme_folder_counts() {
        let uq = Path::new("/s/uq");
        assert!(touches_tokens(uq, Path::new("/s/uq/theme")));
        assert!(touches_tokens(uq, Path::new("/s/uq/theme/tokens.json")));
        assert!(!touches_tokens(uq, Path::new("/s/uq/theme.tmp-1")));
        assert!(!touches_tokens(uq, Path::new("/s/uq/other")));
    }
}
