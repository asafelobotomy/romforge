//! uQ's theme for the window (uq-tokens' Tauri v2 recipe): `uq_tokens`
//! hands the page uQ's `tokens.json` as text (the core reads it, capped;
//! the vendored `uq-tokens.js` checks every value before it becomes CSS),
//! and a watch on uQ's state folder says "uq-tokens-changed" when the
//! theme changes. uQ swaps the whole `theme/` folder at once, so the watch
//! is on its parent. Without uQ there is no folder to watch and the app
//! keeps its own colours (until it is started again).

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use romforge_core::uq::tokens;
use tauri::{AppHandle, Emitter, Manager};

pub const CHANGED: &str = "uq-tokens-changed";

/// uQ's `tokens.json`, or none without uQ (or with an unreadable file).
#[tauri::command]
pub fn uq_tokens() -> Option<String> {
    tokens::read()
}

/// Kept for as long as the app runs.
struct Watching(#[allow(dead_code)] std::sync::Mutex<RecommendedWatcher>);

/// Watch uQ's state folder, when there is one.
pub fn watch(app: &AppHandle) {
    let Some(dir) = tokens::dir().filter(|d| d.is_dir()) else {
        return;
    };
    let handle = app.clone();
    let theme_dir = dir.clone();
    let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok_and(|ev| ev.paths.iter().any(|p| tokens::touches_tokens(&theme_dir, p))) {
            let _ = handle.emit(CHANGED, ());
        }
    });
    let Ok(mut watcher) = watcher else { return };
    if watcher.watch(&dir, RecursiveMode::NonRecursive).is_ok() {
        app.manage(Watching(std::sync::Mutex::new(watcher)));
    }
}
