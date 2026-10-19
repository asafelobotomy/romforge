use std::io::Read;
use std::path::Path;

use sha1::{Digest as _, Sha1};

/// The error [`move_file`] returns when the file is in its new place but
/// the original couldn't be removed: both exist and hold the same bytes.
#[derive(Debug)]
pub(crate) struct SourceLeft(pub std::io::Error);

impl std::fmt::Display for SourceLeft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "moved, but the original couldn't be removed: {}", self.0)
    }
}

impl std::error::Error for SourceLeft {}

/// Whether `e` is a [`SourceLeft`]: the move happened, the original stays.
pub(crate) fn source_left(e: &std::io::Error) -> bool {
    e.get_ref().is_some_and(|inner| inner.is::<SourceLeft>())
}

fn remove_source(from: &Path, size: u64) -> std::io::Result<u64> {
    std::fs::remove_file(from).map_err(|e| std::io::Error::other(SourceLeft(e)))?;
    Ok(size)
}

/// Move `from` to `to` without ever replacing an existing file. Returns the
/// size moved. When the new file is in place but the original couldn't be
/// removed, the error is a [`SourceLeft`] ([`source_left`]).
pub(crate) fn move_file(from: &Path, to: &Path) -> std::io::Result<u64> {
    use std::io::{Error as IoError, ErrorKind};
    let meta = std::fs::symlink_metadata(from)?;
    if meta.file_type().is_symlink() {
        return Err(IoError::other(format!("{} is a symbolic link", from.display())));
    }
    let size = meta.len();
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // A hard link fails if `to` exists, so nothing is clobbered even if a
    // file appears there meanwhile.
    match std::fs::hard_link(from, to) {
        Ok(()) => return remove_source(from, size),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => return Err(e),
        Err(e) if e.kind() == ErrorKind::CrossesDevices => return copy_verify_delete(from, to, size),
        Err(_) => {}
    }
    // Filesystems without hard links (FAT, exFAT, some network shares).
    match rename_noreplace(from, to) {
        Ok(()) => Ok(size),
        Err(e) if e.kind() == ErrorKind::CrossesDevices => copy_verify_delete(from, to, size),
        Err(e) => Err(e),
    }
}

/// Whether two paths hold the same file: the same inode, or two files of
/// the same size and SHA1.
pub(crate) fn same_file(a: &Path, b: &Path) -> std::io::Result<bool> {
    let (ma, mb) = (std::fs::metadata(a)?, std::fs::metadata(b)?);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if ma.dev() == mb.dev() && ma.ino() == mb.ino() {
            return Ok(true);
        }
    }
    Ok(ma.is_file() && mb.is_file() && ma.len() == mb.len() && sha1_file(a)? == sha1_file(b)?)
}

/// Rename that fails if `to` exists. Linux checks atomically
/// (`RENAME_NOREPLACE`); where the filesystem or system can't, a file that
/// appears between the check and the rename could still be replaced.
pub(crate) fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::io::{Error as IoError, ErrorKind};
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        let c = |p: &Path| std::ffi::CString::new(p.as_os_str().as_bytes()).map_err(IoError::other);
        let (f, t) = (c(from)?, c(to)?);
        // SAFETY: both are valid NUL-terminated paths for the call's duration.
        let r = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                f.as_ptr(),
                libc::AT_FDCWD,
                t.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if r == 0 {
            return Ok(());
        }
        let e = IoError::last_os_error();
        // Not supported here (an old kernel, some filesystems): check, then rename.
        if !matches!(e.raw_os_error(), Some(libc::EINVAL | libc::ENOSYS)) {
            return Err(e);
        }
    }
    if to.symlink_metadata().is_ok() {
        return Err(IoError::new(
            ErrorKind::AlreadyExists,
            format!("{} exists", to.display()),
        ));
    }
    std::fs::rename(from, to)
}

/// Make a new directory entry in `dir` durable (no-op where folders can't
/// be opened, as on Windows).
pub(crate) fn sync_dir(dir: &Path) {
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
}

pub(super) fn copy_verify_delete(from: &Path, to: &Path, size: u64) -> std::io::Result<u64> {
    let name = to
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = to.with_file_name(crate::layout::part(&name));
    let result = copy_checked(from, &tmp).and_then(|()| {
        if std::fs::metadata(&tmp)?.len() != size {
            return Err(std::io::Error::other("copy does not match the original"));
        }
        rename_noreplace(&tmp, to)
    });
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    // The copy is on disk under its name before the original goes.
    if let Some(dir) = to.parent() {
        sync_dir(dir);
    }
    remove_source(from, size)
}

/// Copy `from` into the new file `to` (which must not exist), keeping its
/// times and permissions, synced and checked byte for byte. On failure `to`
/// is removed.
pub(crate) fn copy_checked(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut made = false;
    let result = (|| {
        {
            let mut src = std::fs::File::open(from)?;
            let meta = src.metadata()?;
            let mut out = std::fs::File::create_new(to)?;
            made = true;
            std::io::copy(&mut src, &mut out)?;
            // Keep the file's times and permissions, as a rename would.
            let mut times = std::fs::FileTimes::new().set_modified(meta.modified()?);
            if let Ok(accessed) = meta.accessed() {
                times = times.set_accessed(accessed);
            }
            out.set_times(times)?;
            out.set_permissions(meta.permissions())?;
            out.sync_all()?;
        }
        if sha1_file(from)? != sha1_file(to)? {
            return Err(std::io::Error::other("copy does not match the original"));
        }
        Ok(())
    })();
    if result.is_err() && made {
        let _ = std::fs::remove_file(to);
    }
    result
}

fn sha1_file(path: &Path) -> std::io::Result<[u8; 20]> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha1::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().into())
}
