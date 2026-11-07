//! Stand-in emulators for the launch tests: scripts that record their
//! arguments, cores and Flatpaks that are only files.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use romforge_core::launch::Env;
use romforge_core::{Library, Scope};

use super::fixtures;

pub fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

pub struct Fake {
    pub _dir: tempfile::TempDir,
    pub root: PathBuf,
    pub env: Env,
}

/// An empty environment to install stand-ins into.
pub fn fake() -> Fake {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    for d in ["bin", "cores", "flatpak"] {
        std::fs::create_dir(root.join(d)).unwrap();
    }
    let env = Env {
        path: vec![root.join("bin")],
        core_dirs: vec![root.join("cores")],
        flatpak_dirs: vec![root.join("flatpak")],
        ..Default::default()
    };
    Fake { _dir: dir, root, env }
}

/// `id` installed as a Flatpak from Flathub.
pub fn flatpak(f: &Fake, id: &str) {
    let active = f.root.join("flatpak").join(id).join("current/active");
    std::fs::create_dir_all(&active).unwrap();
    std::fs::write(active.join("deploy"), b"flathub\0").unwrap();
    std::fs::create_dir_all(f.root.join("repo")).unwrap();
    std::fs::write(
        f.root.join("repo/config"),
        "[remote \"flathub\"]\nurl=https://dl.flathub.org/repo/\n",
    )
    .unwrap();
}

pub fn core(f: &Fake, name: &str) -> PathBuf {
    let p = f
        .root
        .join("cores")
        .join(format!("{name}_libretro.{}", std::env::consts::DLL_EXTENSION));
    std::fs::write(&p, b"").unwrap();
    p
}

pub fn library() -> Library {
    let mut lib = Library::open_in_memory().unwrap();
    for e in std::fs::read_dir(fixtures().join("dats")).unwrap() {
        lib.import_dat(e.unwrap().path(), None).unwrap();
    }
    lib.scan(fixtures().join("roms"), Scope::All).unwrap();
    lib
}
