//! The fullscreen mode's themes (stage 2, step 4): the `theme://` scheme
//! that serves them ([`romforge_core::themes::file`]: checked, CSS scoped,
//! nothing outside a theme's folder) and the picker's commands.

use std::path::PathBuf;
use std::sync::RwLock;

use romforge_core::Library;
use romforge_core::themes::remote::{self, Remote, Source, Status};
use romforge_core::themes::{self, Applied, Entry, Selection, TEXT_SIZES};
use tauri_plugin_opener::OpenerExt;

use crate::db::{Db, Res, blocking, edit, read};
use crate::jobs::Job;

/// Where the user's themes are, for the scheme (it never waits on the
/// database). Follows the RomForge folder as the art roots do.
static DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

pub fn set_dir(lib: &Library) {
    *DIR.write().unwrap_or_else(|e| e.into_inner()) = lib.themes_dir();
}

fn dir() -> Option<PathBuf> {
    DIR.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Where the community themes' previews are kept.
fn previews() -> PathBuf {
    romforge_core::enrich::default_cache_dir().join("theme-previews")
}

/// `theme://localhost/<id>/<file>`, and the community themes' previews at
/// `theme://localhost/.preview/<file>`.
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
    if let Some(name) = path.strip_prefix(".preview/") {
        let plain = !name.is_empty()
            && name.ends_with(".png")
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-.".contains(&b))
            && !name.contains("..");
        return match std::fs::read(previews().join(name)).ok().filter(|_| plain) {
            Some(body) => respond(200, "image/png", body),
            None => respond(404, "text/plain", Vec::new()),
        };
    }
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

/// A community theme and what it is here.
#[derive(serde::Serialize)]
pub struct Offer {
    #[serde(flatten)]
    pub remote: Remote,
    pub status: Status,
}

#[derive(serde::Serialize)]
pub struct Available {
    /// Where the index came from.
    pub source: String,
    pub themes: Vec<Offer>,
}

/// The community index (romforge-themes) and what each of its themes is
/// here. Fetched each time the list opens; nothing is downloaded.
#[tauri::command]
pub async fn themes_available() -> Res<Available> {
    let dir = dir();
    blocking(move || {
        let source = Source::from_env();
        let index = source.index()?;
        Ok(Available {
            source: source.describe(),
            themes: index
                .themes
                .into_iter()
                .map(|r| Offer {
                    status: remote::status(dir.as_deref(), &r),
                    remote: r,
                })
                .collect(),
        })
    })
    .await
}

fn offered(id: &str) -> Result<(Source, Remote), romforge_core::Error> {
    let source = Source::from_env();
    let r = source
        .index()?
        .themes
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| romforge_core::Error::Settings(format!("{id:?} isn't in the theme index")))?;
    Ok((source, r))
}

/// Download (or update) community theme `id`, as a job. The window has
/// shown its licence and the user accepted it first. `true` once in (the
/// queue reads a job's null as failed).
#[tauri::command]
pub async fn theme_install(app: tauri::AppHandle, id: String, job: Option<u64>) -> Res<bool> {
    let d = dir().ok_or("no folder for themes")?;
    let mut job = Job::start(&app, job);
    blocking(move || {
        let (source, r) = offered(&id)?;
        remote::install(&d, &source, &r, |done, total| {
            job.step(romforge_core::job::Step::new(done, total, r.name.clone()));
        })?;
        Ok(true)
    })
    .await
}

/// Remove a theme romforge downloaded.
#[tauri::command]
pub async fn theme_remove(id: String) -> Res<()> {
    let d = dir().ok_or("no folder for themes")?;
    blocking(move || remote::remove(&d, &id)).await
}

/// A community theme's preview, for an <img> (fetched once).
#[tauri::command]
pub async fn theme_preview(id: String) -> Res<Option<String>> {
    blocking(move || {
        let (source, r) = offered(&id)?;
        Ok(remote::preview(&source, &r, &previews())?.and_then(|p| {
            p.file_name()
                .map(|n| format!("theme://localhost/.preview/{}", n.to_string_lossy()))
        }))
    })
    .await
}
