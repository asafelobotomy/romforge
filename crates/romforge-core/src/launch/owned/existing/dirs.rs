//! The user's own folders, where emulators keep their usual setup.

use std::path::{Path, PathBuf};

/// The user's home and XDG folders: where emulators keep their usual
/// setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDirs {
    pub home: PathBuf,
    pub xdg: Xdg,
}

impl UserDirs {
    /// From `$HOME` and the `$XDG_*` romforge runs with.
    pub fn detect() -> Option<UserDirs> {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        Some(UserDirs {
            xdg: Xdg::detect(&home),
            home,
        })
    }
}

/// The XDG base folders romforge itself runs with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Xdg {
    pub config: PathBuf,
    pub data: PathBuf,
}

impl Xdg {
    pub fn detect(home_dir: &Path) -> Xdg {
        let var = |v: &str, fallback: &str| {
            std::env::var_os(v)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home_dir.join(fallback))
        };
        Xdg {
            config: var("XDG_CONFIG_HOME", ".config"),
            data: var("XDG_DATA_HOME", ".local/share"),
        }
    }
}
