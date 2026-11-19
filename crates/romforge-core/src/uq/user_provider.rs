//! "Add to uQ search" for a romforge that can't install into `/usr` (the
//! AppImage): the launcher provider as the user's own files, with the
//! user's say-so. Three files, all under the user's data folder:
//!
//! - a copy of `romforge-provider` at a stable path (an AppImage is
//!   mounted somewhere new on each run), `romforge/provider/`;
//! - `uq/launcher/romforge.provider`, which uQ reads from the user's
//!   folder;
//! - `dbus-1/services/org.romforge.Provider.service`, whose `Exec=` is that
//!   copy, which lets the session bus start it when uQ's launcher opens
//!   (uQ asks the bus which names it can start, so the user's own
//!   services folder counts).
//!
//! The two text files start with [`MARK`]; removing only takes files that
//! do, and nothing is added next to a package that installed the provider.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::Error;

/// The bus name, the provider's own (`org.romforge.Provider`).
pub const BUS_NAME: &str = "org.romforge.Provider";

/// First line of the two files romforge writes, and how it knows them.
pub const MARK: &str = "# Added by romforge (Settings, uQ search). Remove it there, or delete this file.";

const BIN_NAME: &str = "romforge-provider";

/// Where the user's files go: the data folder (`$XDG_DATA_HOME`, else
/// `~/.local/share`), and the folders a package would have installed to.
#[derive(Debug, Clone)]
pub struct Layout {
    pub data: PathBuf,
    pub system_data: Vec<PathBuf>,
}

impl Layout {
    pub fn current() -> Option<Layout> {
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
        let dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
        let mut system_data: Vec<PathBuf> = dirs
            .split(':')
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .collect();
        for d in ["/usr/local/share", "/usr/share"] {
            if !system_data.iter().any(|s| s == Path::new(d)) {
                system_data.push(d.into());
            }
        }
        Some(Layout { data, system_data })
    }

    pub fn binary(&self) -> PathBuf {
        self.data.join("romforge/provider").join(BIN_NAME)
    }
    pub fn provider_file(&self) -> PathBuf {
        self.data.join("uq/launcher/romforge.provider")
    }
    pub fn service_file(&self) -> PathBuf {
        self.data.join(format!("dbus-1/services/{BUS_NAME}.service"))
    }

    /// A package (deb, rpm, AUR) put the provider in the system's folders.
    pub fn packaged(&self) -> bool {
        self.system_data.iter().any(|d| {
            d.join("uq/launcher/romforge.provider").is_file()
                || d.join(format!("dbus-1/services/{BUS_NAME}.service")).is_file()
        })
    }
}

/// What Settings shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    /// A package installed it: nothing to add.
    pub packaged: bool,
    /// The user's files are there (all three, and romforge's own).
    pub added: bool,
    /// Added, but the copy isn't this romforge's provider.
    pub stale: bool,
    /// The provider this romforge carries (adding copies it); none for a
    /// build without one beside it.
    pub source: Option<PathBuf>,
    /// The files adding writes or removing takes.
    pub files: Vec<PathBuf>,
}

/// The provider this romforge carries: the AppImage's own
/// (`$APPDIR/usr/bin`), else the one beside this program.
pub fn source() -> Option<PathBuf> {
    let beside_appdir = std::env::var_os("APPDIR").map(|d| PathBuf::from(d).join("usr/bin").join(BIN_NAME));
    let beside_exe = std::env::current_exe().ok().map(|e| e.with_file_name(BIN_NAME));
    beside_appdir.into_iter().chain(beside_exe).find(|p| p.is_file())
}

fn ours(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|t| t.lines().next() == Some(MARK))
}

fn same_bytes(a: &Path, b: &Path) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(x), Ok(y)) if x.len() == y.len() => {
            matches!((std::fs::read(a), std::fs::read(b)), (Ok(p), Ok(q)) if p == q)
        }
        _ => false,
    }
}

pub fn provider_text() -> String {
    format!("{MARK}\n[Launcher Provider]\nBusName={BUS_NAME}\nName=romforge\nActivatable=true\n")
}

/// `exec` must survive the bus's `Exec=` line, which splits on spaces and
/// has no quoting worth trusting.
fn service_text(exec: &Path) -> Result<String, Error> {
    let s = exec.to_str().filter(|s| {
        !s.is_empty()
            && !s.chars().any(|c| {
                c.is_whitespace() || c.is_control() || c == '"' || c == '\'' || c == '\\' || c == '%'
            })
    });
    let Some(s) = s else {
        return Err(Error::Settings(format!(
            "{} has a space or an odd character in it, which the D-Bus service file can't hold; set XDG_DATA_HOME to a plainer folder",
            exec.display()
        )));
    };
    Ok(format!("{MARK}\n[D-BUS Service]\nName={BUS_NAME}\nExec={s}\n"))
}

pub fn status_in(layout: &Layout, source: Option<&Path>) -> Status {
    let (bin, prov, svc) = (layout.binary(), layout.provider_file(), layout.service_file());
    let added = bin.is_file() && ours(&prov) && ours(&svc);
    Status {
        packaged: layout.packaged(),
        added,
        stale: added && source.is_some_and(|s| !same_bytes(s, &bin)),
        source: source.map(Path::to_path_buf),
        files: vec![bin, prov, svc],
    }
}

/// Add (or bring up to date) the three files from `source`. Refused next
/// to a package's, and over a file of the user's own.
pub fn add_in(layout: &Layout, source: &Path) -> Result<(), Error> {
    if layout.packaged() {
        return Err(Error::Settings(
            "the package already installed romforge's uQ search".into(),
        ));
    }
    if !source.is_file() {
        return Err(Error::Settings(format!(
            "no {BIN_NAME} to copy: {} isn't there",
            source.display()
        )));
    }
    let (bin, prov, svc) = (layout.binary(), layout.provider_file(), layout.service_file());
    for f in [&prov, &svc] {
        if f.exists() && !ours(f) {
            return Err(Error::Settings(format!(
                "{} is yours, not romforge's: left alone",
                f.display()
            )));
        }
    }
    let service = service_text(&bin)?;
    if let Some(dir) = bin.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::atomic::write_new(&bin, |mut out| {
        std::io::copy(&mut std::fs::File::open(source)?, &mut out)?;
        Ok(())
    })?;
    set_executable(&bin)?;
    crate::atomic::write(&prov, provider_text().as_bytes())?;
    // The service file last: the bus can only start a copy that is there.
    crate::atomic::write(&svc, service.as_bytes())?;
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}
#[cfg(not(unix))]
fn set_executable(_: &Path) -> Result<(), Error> {
    Ok(())
}

/// Take away what adding wrote, and only that. True when anything went.
pub fn remove_in(layout: &Layout) -> Result<bool, Error> {
    let mut any = false;
    for f in [layout.service_file(), layout.provider_file()] {
        if ours(&f) {
            std::fs::remove_file(&f)?;
            any = true;
        }
    }
    let bin = layout.binary();
    if bin.is_file() {
        std::fs::remove_file(&bin)?;
        // Only if nothing else of ours is in there.
        let _ = std::fs::remove_dir(bin.parent().unwrap_or(&layout.data));
        any = true;
    }
    Ok(any)
}

/// A new romforge (a new AppImage) carries a new provider: when the files
/// are added and the copy differs, copy again. Nothing for what isn't
/// added, so this never adds on its own.
pub fn refresh_in(layout: &Layout, source: &Path) -> Result<bool, Error> {
    let st = status_in(layout, Some(source));
    if st.added && st.stale {
        add_in(layout, source)?;
        return Ok(true);
    }
    Ok(false)
}

pub fn status() -> Option<Status> {
    Some(status_in(&Layout::current()?, source().as_deref()))
}

pub fn add() -> Result<(), Error> {
    let layout = Layout::current().ok_or_else(|| Error::Settings("no home folder".into()))?;
    let src = source().ok_or_else(|| {
        Error::Settings(format!(
            "this romforge has no {BIN_NAME} to add (the AppImage and the packages carry one)"
        ))
    })?;
    add_in(&layout, &src)
}

pub fn remove() -> Result<bool, Error> {
    remove_in(&Layout::current().ok_or_else(|| Error::Settings("no home folder".into()))?)
}

pub fn refresh() -> Result<bool, Error> {
    match (Layout::current(), source()) {
        (Some(l), Some(s)) => refresh_in(&l, &s),
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests;
