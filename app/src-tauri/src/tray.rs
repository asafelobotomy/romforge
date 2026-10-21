//! romforge's window while a game plays (Settings' "While a game plays"):
//! hidden with a tray icon to bring it back (the default), minimised, or
//! left open; and brought back when the game ends. Showing a hidden window
//! again is more likely to put it in front on Wayland than raising one.
//!
//! The tray icon is there only while a game runs. On Linux a tray icon
//! reports no clicks (AppIndicator), so its menu is the way back. Where no
//! tray can be made (no StatusNotifier host), the window is minimised.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::db::Db;
use crate::play::Now;

const TRAY: &str = "playing";

fn window(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    app.get_webview_window("main")
}

/// The window out of the way while `game` plays, as Settings says.
#[tauri::command]
pub async fn tuck_away(app: AppHandle, db: tauri::State<'_, Db>, game: String) -> Result<(), String> {
    let how = crate::db::read(&db, |lib| Ok(lib.settings()?.while_playing)).await?;
    // Already over (it closed straight away): the window stays.
    if !running(&app) {
        return Ok(());
    }
    let Some(win) = window(&app) else { return Ok(()) };
    let done = match how.as_str() {
        "stay" => return Ok(()),
        "tray" if tray_library() && tray(&app, &game).is_ok() => win.hide(),
        // "minimise", or no tray to come back from.
        _ => win.minimize(),
    };
    // It ended while the window went: back again.
    if !running(&app) {
        game_ended(&app);
    }
    done.map_err(|e| e.to_string())
}

/// The tray icon for a game that is playing: Show romforge, Stop game.
fn tray(app: &AppHandle, game: &str) -> tauri::Result<()> {
    if app.tray_by_id(TRAY).is_some() {
        return Ok(());
    }
    let show = MenuItem::with_id(app, "show", "Show romforge", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", format!("Stop {game}"), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &stop])?;
    let mut tray = TrayIconBuilder::with_id(TRAY)
        .tooltip(format!("romforge: playing {game}"))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_window(app),
            "stop" => stop_game(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// The window back in front (the tray icon stays while the game runs).
fn show_window(app: &AppHandle) {
    if let Some(win) = window(app) {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// Whether the library a tray icon needs on Linux is there (Tauri loads
/// it when the first tray icon is made, and can't go on without it).
fn tray_library() -> bool {
    if !cfg!(target_os = "linux") {
        return true;
    }
    let names = [
        "libayatana-appindicator3.so.1",
        "libayatana-appindicator3.so",
        "libappindicator3.so.1",
        "libappindicator3.so",
    ];
    let mut dirs: Vec<std::path::PathBuf> = std::env::var_os("LD_LIBRARY_PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    // An AppImage carries its own.
    if let Some(app) = std::env::var_os("APPDIR") {
        dirs.push(std::path::Path::new(&app).join("usr/lib"));
    }
    for d in [
        "/usr/lib",
        "/usr/lib64",
        "/usr/lib/x86_64-linux-gnu",
        "/usr/lib/aarch64-linux-gnu",
        "/lib/x86_64-linux-gnu",
    ] {
        dirs.push(d.into());
    }
    dirs.iter().any(|d| names.iter().any(|n| d.join(n).exists()))
}

fn running(app: &AppHandle) -> bool {
    let now = app.state::<Now>();
    let s = now.0.lock().unwrap_or_else(|e| e.into_inner()).clone();
    s.is_some_and(|s| s.is_running())
}

/// The game asked to quit, from the tray (as the bar's Stop does).
fn stop_game(app: &AppHandle) {
    let now = app.state::<Now>();
    let session = now.0.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(s) = session {
        std::thread::spawn(move || s.stop(crate::play::GRACE));
    }
}

/// The game ended: the window back, the tray icon gone.
pub fn game_ended(app: &AppHandle) {
    let _ = app.remove_tray_by_id(TRAY);
    show_window(app);
}
