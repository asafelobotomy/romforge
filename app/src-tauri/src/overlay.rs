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
    if in_gamescope() {
        fit_gamescope_output(app);
    }
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
        if in_gamescope() {
            // The game's output may have changed (a nested gamescope's
            // window resized): measure it again and follow it.
            fit_gamescope_output(&handle);
        }
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
/// its X11 then, with gamescope's `WAYLAND_DISPLAY` left in place).
fn in_gamescope() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok_and(|w| w.starts_with("gamescope"))
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

/// gamescope's output size in pixels, last measured.
static OUTPUT: std::sync::Mutex<Option<(u32, u32)>> = std::sync::Mutex::new(None);

/// gamescope draws an external overlay unscaled, in its output's pixels,
/// from the output's top-left corner. Its X screen (and so the monitor GDK
/// reports) is the game's resolution (`-w`/`-h`), which is smaller than the
/// output when the game is upscaled (720p on a 1080p screen): a window that
/// size put the ring up and to the left of the middle. Measure the output
/// off the main thread and size the window to it, so the ring (in the
/// middle of the page) is in the middle of the screen.
fn fit_gamescope_output(app: &AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        let Some(size) = gamescope_output() else {
            return;
        };
        *OUTPUT.lock().unwrap_or_else(|e| e.into_inner()) = Some(size);
        let again = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            if let Some(win) = again.get_webview_window(LABEL) {
                resize_to(&win, size);
            }
        });
    });
}

fn resize_to(win: &tauri::WebviewWindow, (w, h): (u32, u32)) {
    let now = win.inner_size().ok().map(|s| (s.width, s.height));
    if now != Some((w, h)) {
        let _ = win.set_size(tauri::PhysicalSize::new(w, h));
        let _ = win.set_position(tauri::PhysicalPosition::new(0, 0));
    }
}

/// gamescope's output size: `ROMFORGE_OVERLAY_SIZE` (`WIDTHxHEIGHT`) when
/// set, else measured from a screenshot gamescope takes at the output's
/// size (`gamescopectl screenshot PATH 2`: every real layer, which is
/// drawn at the output's size; type 1 would be the game's own size). Only
/// the PNG's header is read; the file, in `$XDG_RUNTIME_DIR`, is removed.
/// gamescope publishes the size nowhere else a client can read: its X
/// screen and its Wayland output are both the game's resolution.
fn gamescope_output() -> Option<(u32, u32)> {
    if let Some(size) = std::env::var("ROMFORGE_OVERLAY_SIZE")
        .ok()
        .and_then(|v| parse_size(&v))
    {
        return Some(size);
    }
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = dir.join(format!("romforge-overlay-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let ran = Command::new("gamescopectl")
        .arg("screenshot")
        .arg(&path)
        .arg("2")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !ran {
        return None;
    }
    // gamescope writes it on its own thread a frame or so later.
    let deadline = Instant::now() + Duration::from_secs(3);
    let size = loop {
        let mut head = [0u8; 24];
        if let Ok(mut f) = std::fs::File::open(&path)
            && f.read_exact(&mut head).is_ok()
            && let Some(size) = png_size(&head)
        {
            break Some(size);
        }
        if Instant::now() > deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(40));
    };
    let _ = std::fs::remove_file(&path);
    size
}

/// `1920x1080` as (1920, 1080).
fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.trim().split_once(['x', 'X'])?;
    let (w, h) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

/// A PNG's width and height, from its first 24 bytes (the signature, then
/// the IHDR chunk).
fn png_size(head: &[u8]) -> Option<(u32, u32)> {
    if head.len() < 24 || &head[..8] != b"\x89PNG\r\n\x1a\n" || &head[12..16] != b"IHDR" {
        return None;
    }
    let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
    let (w, h) = (be(&head[16..20]), be(&head[20..24]));
    (w > 0 && h > 0).then_some((w, h))
}

fn build(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    let layered = layer::supported();
    // In gamescope the ring is an external overlay, which gamescope draws
    // with its alpha, unscaled and at the output's origin whatever the
    // window's position (as MangoHud's mangoapp): a transparent window as
    // big as the output, at 0,0, like the layer-shell one. Until the
    // output is measured (`fit_gamescope_output`), the X screen's size.
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
    if gamescope && let Some(size) = *OUTPUT.lock().unwrap_or_else(|e| e.into_inner()) {
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
    use super::{parse_size, png_size};

    #[test]
    fn sizes_parse() {
        assert_eq!(parse_size("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_size(" 2560X1440 "), Some((2560, 1440)));
        assert_eq!(parse_size("1920"), None);
        assert_eq!(parse_size("0x1080"), None);
        assert_eq!(parse_size("wide x tall"), None);
    }

    #[test]
    fn png_header_gives_the_size() {
        let mut head = Vec::from(*b"\x89PNG\r\n\x1a\n");
        head.extend_from_slice(&13u32.to_be_bytes());
        head.extend_from_slice(b"IHDR");
        head.extend_from_slice(&1920u32.to_be_bytes());
        head.extend_from_slice(&1080u32.to_be_bytes());
        assert_eq!(png_size(&head), Some((1920, 1080)));
        assert_eq!(png_size(&head[..20]), None, "too short");
        head[1] = b'J';
        assert_eq!(png_size(&head), None, "not a PNG");
    }
}
