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
//! while it shows, so it is only the fallback.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "overlay";

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
        // Clicks go through to the game. Only once shown: tao unwraps the
        // GDK window, which a hidden window doesn't have yet.
        let _ = win.set_ignore_cursor_events(true);
    });
}

fn build(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    let layered = layer::supported();
    let win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("overlay.html".into()))
        .title("romforge overlay")
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .resizable(false)
        .fullscreen(!layered)
        .shadow(false)
        .visible(false)
        .build()?;
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
