use std::io::Read;
use std::path::Path;

use sha1::{Digest as _, Sha1};

/// Move `from` to `to` without ever replacing an existing file. Returns the
/// size moved.
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
        Ok(()) => {
            std::fs::remove_file(from)?;
            return Ok(size);
        }
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

/// Rename that fails if `to` exists. Linux checks atomically
/// (`RENAME_NOREPLACE`); where the filesystem or system can't, a file that
/// appears between the check and the rename could still be replaced.
pub(super) fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
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
pub(super) fn sync_dir(dir: &Path) {
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
}

pub(super) fn copy_verify_delete(from: &Path, to: &Path, size: u64) -> std::io::Result<u64> {
    use std::io::Error as IoError;
    let name = to
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = to.with_file_name(crate::layout::part(&name));
    let result = (|| {
        {
            let mut src = std::fs::File::open(from)?;
            let meta = src.metadata()?;
            let mut out = std::fs::File::create_new(&tmp)?;
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
        if sha1_file(from)? != sha1_file(&tmp)? || std::fs::metadata(&tmp)?.len() != size {
            return Err(IoError::other("copy does not match the original"));
        }
        rename_noreplace(&tmp, to)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    // The copy is on disk under its name before the original goes.
    if let Some(dir) = to.parent() {
        sync_dir(dir);
    }
    std::fs::remove_file(from)?;
    Ok(size)
}

pub(super) fn sha1_file(path: &Path) -> std::io::Result<[u8; 20]> {
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
