//! How the app was started (`romforge-app --fullscreen` opens its
//! fullscreen mode, for a pad and a TV) and quitting it from that mode.

/// What the command line asked for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct StartOptions {
    pub fullscreen: bool,
    /// `ROMFORGE_ATTRACT_SECONDS`: attract mode after this many seconds,
    /// whatever Settings says (for trying it out).
    pub attract_seconds: Option<u32>,
}

/// Read `args` (without the program's name). Unknown ones are left alone:
/// Steam and desktop launchers add their own.
pub fn start_options<S: AsRef<str>>(args: impl IntoIterator<Item = S>) -> StartOptions {
    StartOptions {
        fullscreen: args
            .into_iter()
            .any(|a| matches!(a.as_ref(), "--fullscreen" | "-f")),
        attract_seconds: std::env::var("ROMFORGE_ATTRACT_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|&n| n > 0),
    }
}

/// How this run was started, for the page.
#[tauri::command]
pub fn start_options_now(opts: tauri::State<'_, StartOptions>) -> StartOptions {
    *opts
}

/// Quit romforge (the fullscreen mode's menu).
#[tauri::command]
pub fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_fullscreen_and_ignores_the_rest() {
        assert!(start_options(["--fullscreen"]).fullscreen);
        assert!(start_options(["-f"]).fullscreen);
        assert!(!start_options(["--something-steam-adds"]).fullscreen);
        assert!(!start_options(Vec::<String>::new()).fullscreen);
    }
}
