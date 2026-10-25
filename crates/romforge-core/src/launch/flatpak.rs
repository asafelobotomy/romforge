//! Whether an installed Flatpak is the emulator's official one
//! ([`FlatpakFrom`]): where it was installed from, read from Flatpak's own
//! files (no program is run). The app's `current/active/deploy` starts with
//! the name of the remote it came from; the installation's `repo/config`
//! gives that remote's URL.

use std::path::Path;

use super::{Env, FlatpakFrom, KnownEmulator, Runner};

/// Flathub's repository.
pub(super) const FLATHUB_URL: &str = "https://dl.flathub.org/repo/";

/// The URL of the remote the app in `app` (`<installation>/app/<id>`) was
/// installed from.
fn origin(app: &Path) -> Option<String> {
    let deploy = std::fs::read(app.join("current/active/deploy")).ok()?;
    let name = std::str::from_utf8(deploy.split(|&b| b == 0).next()?).ok()?;
    let config = std::fs::read_to_string(app.parent()?.parent()?.join("repo/config")).ok()?;
    let header = format!("[remote \"{name}\"]");
    let mut lines = config.lines().skip_while(|l| l.trim() != header).skip(1);
    lines
        .by_ref()
        .take_while(|l| !l.trim_start().starts_with('['))
        .find_map(|l| l.trim().strip_prefix("url=").map(|u| u.trim().to_owned()))
}

/// The URL `e`'s Flatpak must come from; `None`: none is official.
fn wanted(e: &KnownEmulator) -> Option<&'static str> {
    match e.flatpak_from {
        FlatpakFrom::Flathub | FlatpakFrom::Fallback => Some(FLATHUB_URL),
        FlatpakFrom::Own { url, .. } => Some(url),
        FlatpakFrom::Others => None,
    }
}

/// The app's own command, from its `metadata`.
fn command(app: &Path) -> Option<String> {
    let m = std::fs::read_to_string(app.join("current/active/metadata")).ok()?;
    m.lines()
        .find_map(|l| l.strip_prefix("command="))
        .map(|c| c.trim().to_owned())
        .filter(|c| !c.is_empty())
}

impl Env {
    /// `e` would run as the fallback Flatpak of an emulator with no
    /// official Linux build ([`FlatpakFrom::Fallback`]).
    pub fn runs_fallback(&self, e: &KnownEmulator) -> bool {
        e.flatpak_from == FlatpakFrom::Fallback && matches!(self.known(e), Some(Runner::Flatpak { .. }))
    }

    /// The installs of `e`'s Flatpak (any origin).
    fn flatpak_apps<'a>(&'a self, e: &KnownEmulator) -> impl Iterator<Item = std::path::PathBuf> + 'a {
        let id = e.flatpak.filter(|id| !id.is_empty());
        self.flatpak_dirs
            .iter()
            .filter_map(move |d| id.map(|id| d.join(id)))
            .filter(|a| a.is_dir())
    }

    /// `e`'s official Flatpak, when one is installed.
    pub(super) fn flatpak(&self, e: &KnownEmulator) -> Option<Runner> {
        let url = wanted(e)?;
        let same = |u: &str| u.trim_end_matches('/') == url.trim_end_matches('/');
        let app = self
            .flatpak_apps(e)
            .find(|a| origin(a).is_some_and(|u| same(&u)))?;
        Some(Runner::Flatpak {
            id: e.flatpak?.to_owned(),
            command: command(&app),
        })
    }

    /// A Flatpak of `e` is installed that isn't its official one (from
    /// another remote, or one only others build): romforge doesn't use it.
    pub fn unofficial_flatpak(&self, e: &KnownEmulator) -> bool {
        self.flatpak_apps(e).next().is_some() && self.flatpak(e).is_none()
    }
}

/// Make `<dir>/<id>` look like a Flatpak installed from `url` (tests).
#[cfg(test)]
pub(crate) fn fake(dir: &Path, id: &str, url: &str) {
    let active = dir.join(id).join("current/active");
    std::fs::create_dir_all(&active).unwrap();
    std::fs::write(active.join("deploy"), b"origin\0rest").unwrap();
    let repo = dir.parent().unwrap().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(
        repo.join("config"),
        format!("[core]\nmode=bare-user\n\n[remote \"origin\"]\nurl={url}\n"),
    )
    .unwrap();
}
