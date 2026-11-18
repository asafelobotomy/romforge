//! Whether an emulator is running now, started by romforge or not: its
//! files must not be moved or replaced under it. Read from `/proc` (no
//! program is run): a process whose executable or first argument is one of
//! the emulator's programs or its AppImage, or one in its Flatpak's cgroup
//! (`app-flatpak-<id>-…scope`).

use std::path::Path;

use super::{KnownEmulator, LaunchSpec};
use crate::Error;

/// Whether `e` has a process running, as far as `/proc` shows.
pub fn running(e: &KnownEmulator) -> bool {
    running_in(Path::new("/proc"), e)
}

fn running_in(proc_dir: &Path, e: &KnownEmulator) -> bool {
    let Ok(entries) = std::fs::read_dir(proc_dir) else {
        return false;
    };
    let appimage = e.appimage.unwrap_or(e.name).to_ascii_lowercase();
    let flatpak = e.flatpak.map(|id| format!("app-flatpak-{id}-"));
    let named = |p: &str| {
        let base = Path::new(p)
            .file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        e.programs.iter().any(|prog| prog.eq_ignore_ascii_case(&base))
            || (base.starts_with(&appimage) && base.ends_with(".appimage"))
    };
    entries.filter_map(Result::ok).any(|p| {
        let dir = p.path();
        if !p
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit())
        {
            return false;
        }
        let exe = std::fs::read_link(dir.join("exe")).ok();
        let argv0 = std::fs::read(dir.join("cmdline")).ok().and_then(|c| {
            c.split(|&b| b == 0)
                .next()
                .map(|a| String::from_utf8_lossy(a).into_owned())
        });
        let boxed = flatpak.as_ref().is_some_and(|f| {
            std::fs::read_to_string(dir.join("cgroup")).is_ok_and(|c| c.contains(f.as_str()))
        });
        boxed || exe.is_some_and(|x| named(&x.to_string_lossy())) || argv0.is_some_and(|a| named(&a))
    })
}

/// Refuse while an emulator romforge knows runs (RetroArch included):
/// the RomForge folder holds its saves, BIOS and games, and its settings
/// point there, so the folder must not move under it.
pub fn none_running() -> Result<(), Error> {
    none_running_in(Path::new("/proc"))
}

fn none_running_in(proc_dir: &Path) -> Result<(), Error> {
    match std::iter::once(&super::RETROARCH)
        .chain(super::KNOWN_EMULATORS)
        .find(|e| running_in(proc_dir, e))
    {
        Some(e) => Err(Error::Settings(format!(
            "{} is running: close it before moving the RomForge folder (its saves and settings are in it)",
            e.label
        ))),
        None => Ok(()),
    }
}

/// Refuse `spec` while its emulator already runs and romforge writes its
/// config first: a second game (started from the app, the launcher or the
/// CLI, each knowing only its own) would rewrite that config under the
/// first, and the first's backup would run while the second writes.
pub(super) fn not_twice(spec: &LaunchSpec) -> Result<(), Error> {
    not_twice_in(Path::new("/proc"), spec)
}

fn not_twice_in(proc_dir: &Path, spec: &LaunchSpec) -> Result<(), Error> {
    let family = spec.emulator.split(':').next().unwrap_or(&spec.emulator);
    match super::known(family) {
        Some(e) if !spec.prepare.writes.is_empty() && running_in(proc_dir, e) => Err(Error::Launch(format!(
            "{} is already running: close it first (romforge writes its settings before each game)",
            e.label
        ))),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_game_of_a_running_emulator_is_refused() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join("50");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("cmdline"), b"/usr/bin/retroarch\0-L\0core.so").unwrap();
        let mut spec = LaunchSpec {
            game: "Game".into(),
            system: "SNES".into(),
            emulator: "retroarch:snes9x".into(),
            rom: "game.sfc".into(),
            program: "/usr/bin/retroarch".into(),
            args: Vec::new(),
            env: Vec::new(),
            prepare: Default::default(),
            skipped: Vec::new(),
        };
        assert!(
            not_twice_in(t.path(), &spec).is_ok(),
            "no config of romforge's to clash"
        );
        spec.prepare.writes.push(super::super::ConfigWrite {
            path: t.path().join("retroarch.cfg"),
            format: super::super::Format::RetroArch,
            keys: Vec::new(),
            remove: Vec::new(),
        });
        let err = not_twice_in(t.path(), &spec).unwrap_err().to_string();
        assert!(err.contains("already running"), "{err}");
        std::fs::remove_dir_all(&d).unwrap();
        assert!(not_twice_in(t.path(), &spec).is_ok());
    }

    #[test]
    fn a_folder_move_waits_for_every_emulator() {
        let t = tempfile::tempdir().unwrap();
        assert!(none_running_in(t.path()).is_ok(), "nothing runs");
        let d = t.path().join("70");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("cmdline"), b"/usr/bin/retroarch\0-L\0core.so").unwrap();
        let err = none_running_in(t.path()).unwrap_err().to_string();
        assert!(err.contains("RetroArch is running"), "{err}");
    }

    #[test]
    fn a_process_by_program_appimage_or_flatpak() {
        let t = tempfile::tempdir().unwrap();
        let rpcs3 = super::super::known("rpcs3").unwrap();
        let proc_with = |pid: &str, cmdline: &[u8], cgroup: &str| {
            let d = t.path().join(pid);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("cmdline"), cmdline).unwrap();
            std::fs::write(d.join("cgroup"), cgroup).unwrap();
        };
        proc_with("1", b"/usr/lib/systemd/systemd\0--user", "0::/init.scope");
        proc_with("self", b"rpcs3\0", "");
        assert!(!running_in(t.path(), rpcs3), "not a pid, or not it");
        proc_with(
            "42",
            b"/home/me/Applications/rpcs3-v0.0.43_linux64.AppImage\0--no-gui",
            "0::/user.slice",
        );
        assert!(running_in(t.path(), rpcs3), "its AppImage");
        std::fs::remove_dir_all(t.path().join("42")).unwrap();
        proc_with(
            "43",
            b"/app/bin/rpcs3-bin\0",
            "0::/user.slice/app-flatpak-net.rpcs3.RPCS3-1234.scope",
        );
        assert!(running_in(t.path(), rpcs3), "its Flatpak");
        std::fs::remove_dir_all(t.path().join("43")).unwrap();
        proc_with("44", b"/usr/bin/rpcs3\0game.iso", "");
        assert!(running_in(t.path(), rpcs3), "its program");
    }
}
