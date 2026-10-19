//! Writing a cache file whole: through a temporary file beside it, named
//! uniquely per process and write, then renamed. A crash never leaves half a
//! file under the real name, and two writers at once never share a
//! temporary file.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::Error;

/// A temporary name beside `path` no other write uses:
/// `.<name>.<pid>-<n>.tmp`.
fn temp_for(path: &Path) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!(
        ".{name}.{}-{}.tmp",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Write `path` by `fill`ing a temporary file beside it, then renaming it.
/// The parent folder must exist. On failure the temporary file goes.
pub(crate) fn write_new(
    path: &Path,
    fill: impl FnOnce(&std::fs::File) -> Result<(), Error>,
) -> Result<(), Error> {
    let tmp = temp_for(path);
    let result = std::fs::File::create(&tmp)
        .map_err(Error::from)
        .and_then(|f| fill(&f))
        .and_then(|()| std::fs::rename(&tmp, path).map_err(Error::from));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Write `data` to `path` whole, making its folder first.
pub(crate) fn write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    write_new(path, |mut f| {
        std::io::Write::write_all(&mut f, data)?;
        Ok(())
    })
    .map_err(|e| match e {
        Error::Io(e) => e,
        e => std::io::Error::other(e.to_string()),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn writes_whole_and_leaves_no_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("a.png");
        super::write(&path, b"one").unwrap();
        super::write(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["a.png"]);
        assert_ne!(super::temp_for(&path), super::temp_for(&path), "unique per write");
    }
}
