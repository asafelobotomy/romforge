//! The radial menu over a game this provider started. The ring is drawn
//! by the romforge app, run as `romforge-app --overlay=EMULATOR` (no
//! window of its own, no library): it is sent the radial menu's events,
//! one JSON line each, and quits when the pipe closes. The provider keeps
//! the pad and does the hotkeys; the app is only asked to draw, so a
//! missing app costs the ring and nothing else.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use romforge_core::hotkeys::HotkeyEvent;

/// Names the app may have been installed as, beside the provider and on
/// `PATH`.
const APP_NAMES: &[&str] = &["romforge-app", "romforge"];

/// Testing aid, and for an app somewhere unusual: the app's program.
pub const APP_ENV: &str = "ROMFORGE_APP";

/// Testing aid and override: `x11` or `wayland` makes the overlay use that
/// display protocol, whatever the session looks like.
pub const BACKEND_ENV: &str = "ROMFORGE_OVERLAY_BACKEND";

/// How long the app gets to quit once its pipe is closed.
const QUIT_WITHIN: Duration = Duration::from_secs(3);

fn is_program(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Where the app is: `$ROMFORGE_APP`, else beside this program, else on
/// `PATH`.
pub fn app_program() -> Option<PathBuf> {
    let env = std::env::var_os(APP_ENV).map(PathBuf::from);
    let beside = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf));
    let on_path: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    find_app(env, beside.into_iter().chain(on_path))
}

/// [`app_program`]'s choice, from the places it looks.
pub fn find_app(env: Option<PathBuf>, dirs: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    if let Some(p) = env {
        // Asked for by name: that one or none.
        return is_program(&p).then_some(p);
    }
    dirs.into_iter()
        .flat_map(|d| APP_NAMES.iter().map(move |n| d.join(n)))
        .find(|p| is_program(p))
}

/// Whether the overlay should be forced onto X11 (`GDK_BACKEND=x11`).
/// gamescope's own Wayland socket has no `wl_subcompositor` (and no
/// layer-shell), and a GTK window there draws nothing, while through its
/// Xwayland it shows. So: `$ROMFORGE_OVERLAY_BACKEND` if it says, else
/// when `WAYLAND_DISPLAY` is gamescope's and there is an X display.
pub fn use_x11(var: impl Fn(&str) -> Option<String>) -> bool {
    match var(BACKEND_ENV).as_deref() {
        Some("x11") => return true,
        Some("wayland") => return false,
        _ => {}
    }
    var("WAYLAND_DISPLAY").is_some_and(|w| w.starts_with("gamescope"))
        && var("DISPLAY").is_some_and(|d| !d.is_empty())
}

/// The app running as the overlay, for one game.
pub struct Overlay {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
}

impl Overlay {
    pub fn start(program: &Path, emulator: &str) -> std::io::Result<Overlay> {
        let mut cmd = Command::new(program);
        cmd.arg(format!("--overlay={emulator}"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null());
        if use_x11(|k| std::env::var(k).ok()) {
            cmd.env("GDK_BACKEND", "x11");
        }
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take();
        Ok(Overlay {
            child: Some(child),
            stdin,
        })
    }

    /// Tell the ring. An error when the app is gone: the ring is then
    /// given up for this game.
    pub fn send(&mut self, ev: &HotkeyEvent) -> std::io::Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
        let mut line = serde_json::to_string(ev).map_err(std::io::Error::other)?;
        line.push('\n');
        stdin.write_all(line.as_bytes())?;
        stdin.flush()
    }
}

impl Drop for Overlay {
    /// The game ended: the pipe closes and the app quits, or is stopped if
    /// it doesn't. Off this thread, so the end of a game isn't held up.
    fn drop(&mut self) {
        self.stdin = None;
        let Some(mut child) = self.child.take() else {
            return;
        };
        std::thread::spawn(move || {
            let by = Instant::now() + QUIT_WITHIN;
            while Instant::now() < by {
                if child.try_wait().is_ok_and(|s| s.is_some()) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = child.kill();
            let _ = child.wait();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn script(path: &Path, body: &str) {
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn gamescope_gets_the_overlay_on_x11() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
        };
        let on = |p| use_x11(env(p));
        assert!(on(&[("WAYLAND_DISPLAY", "gamescope-0"), ("DISPLAY", ":1")]));
        assert!(!on(&[("WAYLAND_DISPLAY", "gamescope-0")]), "no X display to use");
        assert!(!on(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0")]));
        assert!(!on(&[]));
        // Asked for by name, whatever the session is.
        assert!(on(&[(BACKEND_ENV, "x11")]));
        assert!(!on(&[
            (BACKEND_ENV, "wayland"),
            ("WAYLAND_DISPLAY", "gamescope-0"),
            ("DISPLAY", ":1")
        ]));
    }

    #[test]
    fn the_app_is_found_by_name_beside_the_provider_or_on_the_path() {
        let t = tempfile::tempdir().unwrap();
        let (a, b) = (t.path().join("a"), t.path().join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        assert_eq!(find_app(None, [a.clone(), b.clone()]), None);
        // Not executable: not the app.
        std::fs::write(b.join("romforge"), b"").unwrap();
        assert_eq!(find_app(None, [a.clone(), b.clone()]), None);
        script(&b.join("romforge"), "");
        assert_eq!(find_app(None, [a.clone(), b.clone()]), Some(b.join("romforge")));
        // Earlier places and names win.
        script(&a.join("romforge-app"), "");
        assert_eq!(
            find_app(None, [a.clone(), b.clone()]),
            Some(a.join("romforge-app"))
        );
        // Named by $ROMFORGE_APP: that one, or none.
        assert_eq!(
            find_app(Some(b.join("romforge")), [a.clone()]),
            Some(b.join("romforge"))
        );
        assert_eq!(find_app(Some(t.path().join("none")), [a, b]), None);
    }

    #[test]
    fn events_are_json_lines_and_the_app_quits_when_the_pipe_closes() {
        let t = tempfile::tempdir().unwrap();
        let (app, got) = (t.path().join("app"), t.path().join("got"));
        script(&app, &format!("echo \"$1\" > '{0}'\ncat >> '{0}'", got.display()));
        let mut o = Overlay::start(&app, "retroarch:snes9x").unwrap();
        o.send(&HotkeyEvent::RadialOpen).unwrap();
        o.send(&HotkeyEvent::RadialSlot { slot: Some(1) }).unwrap();
        drop(o);
        let by = Instant::now() + Duration::from_secs(5);
        let text = loop {
            let text = std::fs::read_to_string(&got).unwrap_or_default();
            if text.lines().count() >= 3 || Instant::now() > by {
                break text;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        assert_eq!(
            text,
            "--overlay=retroarch:snes9x\n{\"kind\":\"radial_open\"}\n{\"kind\":\"radial_slot\",\"slot\":1}\n"
        );
    }

    #[test]
    fn a_gone_app_is_an_error_not_a_hang() {
        let t = tempfile::tempdir().unwrap();
        let app = t.path().join("app");
        script(&app, "exit 0");
        let mut o = Overlay::start(&app, "x").unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!((0..3).any(|_| {
            let r = o.send(&HotkeyEvent::RadialClose);
            std::thread::sleep(Duration::from_millis(100));
            r.is_err()
        }));
    }
}
