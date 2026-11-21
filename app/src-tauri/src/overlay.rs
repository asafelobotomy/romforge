//! The radial menu's window over a running game (stage 2, step 3). The
//! pad drives it (`hotkeys`), so it never needs the focus.
//!
//! Where the compositor has wlr-layer-shell (KWin, wlroots; not GNOME) and
//! the gtk-layer-shell library is installed, the window is an overlay-layer
//! surface that never takes the keyboard: above the fullscreen game, which
//! keeps its focus and stays fullscreen. The library is loaded at run time,
//! so romforge starts without it. Elsewhere the window is a plain
//! fullscreen one: on KDE Plasma 6.7 Wayland that takes the focus
//! (`focused`/`focusable` are ignored) and the game drops below the panel
//! while it shows, so it is only the fallback; there the ring is a small
//! window in the middle of the screen, not a fullscreen one, so it leaves
//! the game its place where the compositor can't show it transparent
//! (gamescope).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "overlay";

/// The side of the window the ring has where there is no layer-shell.
const PLAIN_SIZE: f64 = 600.0;

/// Make the overlay, hidden, for a game starting (its page loads now, so
/// it is ready when the ring opens).
pub fn prepare(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if handle.get_webview_window(LABEL).is_none()
            && let Err(e) = build(&handle)
        {
            eprintln!("romforge: overlay: {e}");
        }
    });
}

/// Show or hide the overlay.
pub fn show(app: &AppHandle, on: bool) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let win = match handle.get_webview_window(LABEL) {
            Some(w) => w,
            None if on => match build(&handle) {
                Ok(w) => w,
                Err(e) => return eprintln!("romforge: overlay: {e}"),
            },
            None => return,
        };
        if !on {
            let _ = win.hide();
            return;
        }
        let _ = win.show();
        // The window is mapped by the event loop a moment after `show`.
        let again = win.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            let marked = again.clone();
            let _ = again.run_on_main_thread(move || mark_external_overlay(&marked));
        });
        // Clicks go through to the game. Only once shown: tao unwraps the
        // GDK window, which a hidden window doesn't have yet.
        let _ = win.set_ignore_cursor_events(true);
    });
}

/// Whether the session is gamescope's (the provider starts the overlay on
/// its X11 then). gamescope 3.17 nested gives its programs only
/// `GAMESCOPE_WAYLAND_DISPLAY`, no `WAYLAND_DISPLAY`; others set
/// `WAYLAND_DISPLAY` to its socket.
fn in_gamescope() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok_and(|w| w.starts_with("gamescope"))
        || std::env::var("GAMESCOPE_WAYLAND_DISPLAY").is_ok_and(|w| !w.is_empty())
}

/// In gamescope only the focused window and windows marked as an external
/// overlay are drawn, and this one never takes the focus: mark it
/// (`GAMESCOPE_EXTERNAL_OVERLAY` = 1, as MangoHud's does). Only where the
/// session is gamescope's (the provider starts the overlay on its X11
/// then, `romforge-provider`'s `overlay.rs`).
#[cfg(target_os = "linux")]
fn mark_external_overlay(win: &tauri::WebviewWindow) {
    use gtk::gdk;
    use gtk::prelude::WidgetExt;
    if !in_gamescope() {
        return;
    }
    let Some(window) = win.gtk_window().ok().and_then(|w| w.window()) else {
        return;
    };
    gdk::property_change(
        &window,
        &gdk::Atom::intern("GAMESCOPE_EXTERNAL_OVERLAY"),
        &gdk::Atom::intern("CARDINAL"),
        32,
        gdk::PropMode::Replace,
        gdk::ChangeData::ULongs(&[1]),
    );
}

#[cfg(not(target_os = "linux"))]
fn mark_external_overlay(_: &tauri::WebviewWindow) {}

fn resize_to(win: &tauri::WebviewWindow, (w, h): (u32, u32)) {
    let now = win.inner_size().ok().map(|s| (s.width, s.height));
    if now != Some((w, h)) {
        let _ = win.set_size(tauri::PhysicalSize::new(w, h));
        let _ = win.set_position(tauri::PhysicalPosition::new(0, 0));
    }
}

/// gamescope draws an external overlay unscaled, in its output's pixels,
/// from the output's top-left corner. Its X screen (and so the monitor GDK
/// reports) is the game's resolution (`-w`/`-h`), smaller than the output
/// when the game is upscaled (720p on a 1080p screen): a window that size
/// put the ring up and to the left of the middle. gamescope publishes the
/// output's size nowhere a client can read (its X screen, its Wayland
/// output and its screenshots are all the game's size), so it is worked
/// out: `ROMFORGE_OVERLAY_SIZE` (`WIDTHxHEIGHT`) when set; else
/// gamescope's own `-W`/`-H`; else, when it has `-w`/`-h` (and so may be
/// upscaling), the connected screen's preferred mode, which gamescope
/// takes on its own (DRM) and fills when fullscreen. `None`: the X
/// screen's size, which is the output's when there is no `-w`/`-h`.
fn gamescope_output() -> Option<(u32, u32)> {
    if let Some(size) = std::env::var("ROMFORGE_OVERLAY_SIZE")
        .ok()
        .and_then(|v| parse_size(&v))
    {
        return Some(size);
    }
    let args = gamescope_args()?;
    let size = GamescopeSize::from_args(&args);
    if let Some(out) = size.output() {
        return Some(out);
    }
    if size.nested {
        return screen_mode();
    }
    None
}

/// The command line of the gamescope this runs in: the nearest ancestor
/// process named `gamescope`.
fn gamescope_args() -> Option<Vec<String>> {
    let mut pid = std::process::id();
    for _ in 0..64 {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // `pid (comm) state ppid ...`; comm may hold spaces and brackets.
        let ppid: u32 = stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()?;
        if ppid <= 1 {
            return None;
        }
        pid = ppid;
        let Ok(cmdline) = std::fs::read(format!("/proc/{pid}/cmdline")) else {
            continue;
        };
        let args: Vec<String> = cmdline
            .split(|b| *b == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect();
        let name = args.first().map(|a| a.rsplit('/').next().unwrap_or(a));
        if name == Some("gamescope") {
            return Some(args);
        }
    }
    None
}

/// What gamescope's command line says of its sizes.
#[derive(Debug, Default, PartialEq)]
struct GamescopeSize {
    width: Option<u32>,
    height: Option<u32>,
    /// `-w`/`-h` given: the game's resolution may be below the output's.
    nested: bool,
}

impl GamescopeSize {
    /// Read up to `--`, after which is the game's own command line.
    fn from_args(args: &[String]) -> Self {
        let mut size = Self::default();
        let mut it = args.iter().skip(1);
        while let Some(arg) = it.next() {
            if arg == "--" {
                break;
            }
            let (flag, inline) = match arg.split_once('=') {
                Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
                _ => match arg.as_str() {
                    a if a.len() > 2 && a.starts_with('-') && !a.starts_with("--") => {
                        (&a[..2], Some(a[2..].to_string()))
                    }
                    a => (a, None),
                },
            };
            let which = match flag {
                "-W" | "--output-width" => 'W',
                "-H" | "--output-height" => 'H',
                "-w" | "--nested-width" | "-h" | "--nested-height" => 'n',
                _ => continue,
            };
            let value = inline.or_else(|| it.next().cloned());
            let n = value.and_then(|v| v.parse::<u32>().ok()).filter(|n| *n > 0);
            match which {
                'W' => size.width = n,
                'H' => size.height = n,
                _ => size.nested |= n.is_some(),
            }
        }
        size
    }

    /// The output's size as gamescope works it out from `-W`/`-H`: the
    /// width is 16:9 of the height when only that is given.
    fn output(&self) -> Option<(u32, u32)> {
        let h = self.height?;
        Some((self.width.unwrap_or(h * 16 / 9), h))
    }
}

/// The preferred mode of the connected screen (DRM's sysfs: a connector
/// that is connected and enabled, its first mode); the first one found
/// where there are several.
fn screen_mode() -> Option<(u32, u32)> {
    let mut found: Vec<(bool, (u32, u32))> = std::fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok();
            if read("status")?.trim() != "connected" {
                return None;
            }
            let enabled = read("enabled").is_some_and(|s| s.trim() == "enabled");
            let mode = parse_size(read("modes")?.lines().next()?)?;
            Some((enabled, mode))
        })
        .collect();
    found.sort_by_key(|(enabled, _)| !enabled);
    found.first().map(|(_, mode)| *mode)
}

/// `1920x1080` as (1920, 1080); a mode's `1920x1080i` too.
fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.trim().split_once(['x', 'X'])?;
    let h = h.trim().trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let (w, h) = (w.trim().parse().ok()?, h.parse().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

fn build(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    let layered = layer::supported();
    // In gamescope the ring is an external overlay, which gamescope draws
    // with its alpha, unscaled and at the output's origin whatever the
    // window's position (as MangoHud's mangoapp): a transparent window as
    // big as the output (`gamescope_output`), at 0,0, like the layer-shell
    // one.
    let gamescope = !layered && in_gamescope();
    // Elsewhere without layer-shell: a small window in the middle of the
    // screen, on top, with an opaque page (`?plain`), because a fullscreen
    // one takes the game's place.
    let page = if layered || gamescope {
        "overlay.html"
    } else {
        "overlay.html?plain"
    };
    let mut win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(page.into()))
        .title("romforge overlay")
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .resizable(false)
        .shadow(false)
        .visible(false);
    if gamescope {
        let (w, h) = app
            .primary_monitor()
            .ok()
            .flatten()
            .map(|m| {
                let s = m.size().to_logical::<f64>(m.scale_factor());
                (s.width, s.height)
            })
            .unwrap_or((1280.0, 800.0));
        win = win.inner_size(w, h).position(0.0, 0.0);
    } else if !layered {
        win = win.inner_size(PLAIN_SIZE, PLAIN_SIZE).center();
    }
    let win = win.build()?;
    if gamescope && let Some(size) = gamescope_output() {
        resize_to(&win, size);
    }
    if layered {
        #[cfg(target_os = "linux")]
        {
            use gtk::glib::object::ObjectType;
            use gtk::prelude::WidgetExt;
            // GTK has to be told before the window is realised.
            let gtk = win.gtk_window()?;
            if gtk.is_realized() {
                gtk.unrealize();
            }
            layer::make(gtk.as_ptr().cast());
        }
    }
    Ok(win)
}

/// gtk-layer-shell (GTK 3), opened at run time.
#[cfg(target_os = "linux")]
mod layer {
    use std::ffi::{c_char, c_int, c_void};
    use std::sync::OnceLock;

    use libloading::Library;

    // From gtk-layer-shell.h.
    const LAYER_OVERLAY: c_int = 3;
    const KEYBOARD_NONE: c_int = 0;
    const EDGES: [c_int; 4] = [0, 1, 2, 3];

    fn lib() -> Option<&'static Library> {
        static LIB: OnceLock<Option<Library>> = OnceLock::new();
        LIB.get_or_init(|| {
            // SAFETY: a system library made for GTK 3 apps; loading it runs
            // only its own initialisers.
            unsafe { Library::new("libgtk-layer-shell.so.0") }.ok()
        })
        .as_ref()
    }

    /// The library is there and the compositor has layer-shell.
    pub fn supported() -> bool {
        let Some(l) = lib() else { return false };
        // SAFETY: the symbol's C signature is `gboolean (void)`; GTK is
        // initialised (Tauri's setup has run).
        unsafe {
            // Keyboard modes came in 0.6; without them it would take keys.
            l.get::<unsafe extern "C" fn(*mut c_void, c_int)>(b"gtk_layer_set_keyboard_mode\0")
                .is_ok()
                && l.get::<unsafe extern "C" fn() -> c_int>(b"gtk_layer_is_supported\0")
                    .is_ok_and(|f| f() != 0)
        }
    }

    /// Make `window` (a `GtkWindow*`, not yet realised) an overlay-layer
    /// surface over every edge that takes no keyboard.
    pub fn make(window: *mut c_void) {
        let Some(l) = lib() else { return };
        type Win = unsafe extern "C" fn(*mut c_void);
        type WinInt = unsafe extern "C" fn(*mut c_void, c_int);
        type WinStr = unsafe extern "C" fn(*mut c_void, *const c_char);
        type WinIntInt = unsafe extern "C" fn(*mut c_void, c_int, c_int);
        // SAFETY: each signature is the one in gtk-layer-shell.h (enums and
        // gboolean are C ints), and `window` is a live GtkWindow.
        unsafe {
            let (Ok(init), Ok(namespace), Ok(layer), Ok(keyboard), Ok(zone), Ok(anchor)) = (
                l.get::<Win>(b"gtk_layer_init_for_window\0"),
                l.get::<WinStr>(b"gtk_layer_set_namespace\0"),
                l.get::<WinInt>(b"gtk_layer_set_layer\0"),
                l.get::<WinInt>(b"gtk_layer_set_keyboard_mode\0"),
                l.get::<WinInt>(b"gtk_layer_set_exclusive_zone\0"),
                l.get::<WinIntInt>(b"gtk_layer_set_anchor\0"),
            ) else {
                return;
            };
            init(window);
            namespace(window, c"romforge-overlay".as_ptr());
            layer(window, LAYER_OVERLAY);
            keyboard(window, KEYBOARD_NONE);
            zone(window, -1);
            for edge in EDGES {
                anchor(window, edge, 1);
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod layer {
    pub fn supported() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{GamescopeSize, parse_size};

    fn args(line: &str) -> Vec<String> {
        line.split(' ').map(String::from).collect()
    }

    #[test]
    fn sizes_parse() {
        assert_eq!(parse_size("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_size(" 2560X1440 "), Some((2560, 1440)));
        assert_eq!(parse_size("1920x1080i"), Some((1920, 1080)));
        assert_eq!(parse_size("1920"), None);
        assert_eq!(parse_size("0x1080"), None);
        assert_eq!(parse_size("wide x tall"), None);
    }

    #[test]
    fn gamescope_args_give_the_output() {
        let s = |l| GamescopeSize::from_args(&args(l));
        let upscaled = s("gamescope --backend wayland -f -w 1280 -h 720 -W 1920 -H 1080 -- mpv -W 5");
        assert_eq!(upscaled.output(), Some((1920, 1080)));
        assert!(upscaled.nested);
        let joined = s("/usr/bin/gamescope -w1280 -h720 --output-width=2560 --output-height=1440");
        assert_eq!(joined.output(), Some((2560, 1440)));
        // Only -H: 16:9 of it, as gamescope does.
        assert_eq!(s("gamescope -H 1080 -- game").output(), Some((1920, 1080)));
        // -w/-h without -W/-H: the screen's mode decides.
        let nested = s("gamescope -w 1280 -h 720 -f -- game");
        assert_eq!((nested.output(), nested.nested), (None, true));
        // Nothing: the X screen is the output.
        assert_eq!(s("gamescope -e -- steam -gamepadui"), GamescopeSize::default());
        // The game's own flags after `--` are not gamescope's.
        assert_eq!(s("gamescope -- game -H 600").output(), None);
    }
}
