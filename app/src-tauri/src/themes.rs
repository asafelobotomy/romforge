//! The fullscreen mode's themes (stage 2, step 4): the `theme://` scheme
//! that serves them ([`romforge_core::themes::file`]: checked, CSS scoped,
//! nothing outside a theme's folder) and the picker's commands.

use std::path::PathBuf;
use std::sync::RwLock;

use romforge_core::Library;
use romforge_core::themes::{self, Applied, Entry, Selection, TEXT_SIZES};
use tauri_plugin_opener::OpenerExt;

use crate::db::{Db, Res, edit, read};

/// Where the user's themes are, for the scheme (it never waits on the
/// database). Follows the RomForge folder as the art roots do.
static DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

pub fn set_dir(lib: &Library) {
    *DIR.write().unwrap_or_else(|e| e.into_inner()) = lib.themes_dir();
}

fn dir() -> Option<PathBuf> {
    DIR.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// `theme://localhost/<id>/<file>`.
pub fn serve(uri_path: &str) -> tauri::http::Response<Vec<u8>> {
    let respond = |status: u16, mime: &str, body: Vec<u8>| {
        tauri::http::Response::builder()
            .status(status)
            .header("Content-Type", mime)
            .header("Cache-Control", "no-store")
            .header("X-Content-Type-Options", "nosniff")
            .body(body)
            .unwrap_or_default()
    };
    let path = uri_path.trim_start_matches('/');
    let Some((id, file)) = path.split_once('/') else {
        return respond(404, "text/plain", Vec::new());
    };
    match themes::file(dir().as_deref(), id, file) {
        Some((body, mime)) => respond(200, mime, body),
        None => respond(404, "text/plain", Vec::new()),
    }
}

#[derive(serde::Serialize)]
pub struct ThemesView {
    pub themes: Vec<Entry>,
    pub selection: Selection,
    pub applied: Applied,
    pub text_sizes: &'static [u32],
    /// The folder for the user's own themes.
    pub dir: Option<PathBuf>,
}

#[tauri::command]
pub async fn themes_view(db: tauri::State<'_, Db>) -> Res<ThemesView> {
    read(&db, |lib| {
        Ok(ThemesView {
            themes: lib.themes(),
            selection: lib.theme_selection()?,
            applied: lib.theme_applied()?,
            text_sizes: TEXT_SIZES,
            dir: lib.themes_dir(),
        })
    })
    .await
}

/// Keep the pick; returns what the fullscreen mode applies.
#[tauri::command]
pub async fn theme_set(db: tauri::State<'_, Db>, selection: Selection) -> Res<Applied> {
    edit(&db, move |lib| lib.set_theme(&selection)).await
}

/// When theme `id` last changed, for live reload.
#[tauri::command]
pub fn theme_stamp(id: String) -> u64 {
    themes::stamp(dir().as_deref(), &id)
}

/// Open the folder for the user's themes (made first).
#[tauri::command]
pub fn themes_folder(app: tauri::AppHandle) -> Res<()> {
    let d = dir().ok_or("no library folder")?;
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(d.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}
