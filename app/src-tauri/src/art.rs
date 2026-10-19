//! The `art://` scheme: picture files served from the art roots.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use romforge_core::Library;
use romforge_core::enrich::default_cache_dir;
use romforge_core::home::Home;

/// The folders the `art://` scheme may serve pictures from: the cache
/// (art, previews) and the RomForge folder's Media/. Changes when the
/// RomForge folder is made or moved.
#[derive(Clone)]
pub struct ArtRoots(pub Arc<RwLock<Vec<PathBuf>>>);

impl ArtRoots {
    pub fn of(lib: &Library) -> Vec<PathBuf> {
        let mut roots = vec![default_cache_dir()];
        roots.extend(lib.home().map(Home::media));
        // Canonical where they exist already (a folder made later still counts).
        roots.into_iter().map(|r| r.canonicalize().unwrap_or(r)).collect()
    }

    pub fn set(&self, lib: &Library) {
        *self.0.write().unwrap_or_else(|e| e.into_inner()) = Self::of(lib);
    }
}

/// `art://localhost/<file path>`: a picture file under one of the art
/// roots, so the list's picture views load only what is on screen, from
/// disk, without passing it through a command as base64. With `?h=<px>`,
/// a thumbnail that tall (made on first request, kept in the cache), or
/// the picture itself if that fails.
pub fn serve_art(roots: &[PathBuf], uri_path: &str, query: Option<&str>) -> tauri::http::Response<Vec<u8>> {
    let not_found = || {
        tauri::http::Response::builder()
            .status(404)
            .body(Vec::new())
            .unwrap_or_default()
    };
    let mime = |p: &Path| match p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("webp") => Some("image/webp"),
        _ => None,
    };
    let path = PathBuf::from(percent_decode(uri_path));
    if mime(&path).is_none() {
        return not_found();
    }
    let Ok(real) = path.canonicalize() else {
        return not_found();
    };
    if !roots.iter().any(|r| real.starts_with(r)) {
        return not_found();
    }
    let height = query
        .into_iter()
        .flat_map(|q| q.split('&'))
        .find_map(|kv| kv.strip_prefix("h=")?.parse::<u32>().ok());
    let real = height
        .and_then(|h| romforge_core::thumbs::thumbnail(&real, h, &default_cache_dir()).ok())
        .unwrap_or(real);
    let Some(mime) = mime(&real) else {
        return not_found();
    };
    match std::fs::read(&real) {
        Ok(data) => tauri::http::Response::builder()
            .header("Content-Type", mime)
            .header("Cache-Control", "no-cache")
            .body(data)
            .unwrap_or_else(|_| not_found()),
        Err(_) => not_found(),
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
