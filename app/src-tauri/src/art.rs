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
        // Made first, so each is canonical (`/home` may be a link to
        // `/var/home`, and served paths are compared canonical).
        roots
            .into_iter()
            .map(|r| {
                let _ = std::fs::create_dir_all(&r);
                r.canonicalize().unwrap_or(r)
            })
            .collect()
    }

    pub fn set(&self, lib: &Library) {
        *self.0.write().unwrap_or_else(|e| e.into_inner()) = Self::of(lib);
        crate::themes::set_dir(lib);
    }
}

/// `art://localhost/<file path>`: a picture (or a manual's PDF) under one of the art
/// roots, so the list's picture views load only what is on screen, from
/// disk, without passing it through a command as base64. With `?h=<px>`,
/// a thumbnail that tall (made on first request, kept in the cache), or
/// the picture itself if that fails.
pub fn serve_art(roots: &[PathBuf], uri_path: &str, query: Option<&str>) -> tauri::http::Response<Vec<u8>> {
    // fetch() from the page (pdf.js manuals, OCR traineddata) is
    // cross-origin to art://, so CORS headers are required; <img> does not
    // need them. Without Allow-Origin, WebKit fails the fetch as "Load failed".
    let respond = |status: u16, mime: &str, body: Vec<u8>| {
        let headers = [
            ("Content-Type", mime),
            ("Cache-Control", "no-cache"),
            ("Access-Control-Allow-Origin", "*"),
        ];
        response(status, &headers, body)
    };
    let not_found = || respond(404, "application/octet-stream", Vec::new());
    let mime = |p: &Path| match p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("webp") => Some("image/webp"),
        // Video snaps Enrich saved (Settings), played from a Blob.
        Some("mp4") => Some("video/mp4"),
        // Manuals (milestone 26), read with pdf.js.
        Some("pdf") => Some("application/pdf"),
        // Tesseract's language files, read by the OCR worker.
        Some("traineddata") => Some("application/octet-stream"),
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
        Ok(data) => respond(200, mime, data),
        Err(_) => not_found(),
    }
}

/// A scheme's answer (`art://`, `theme://`, `music://`).
pub(crate) fn response(
    status: u16,
    headers: &[(&str, &str)],
    body: Vec<u8>,
) -> tauri::http::Response<Vec<u8>> {
    let mut b = tauri::http::Response::builder().status(status);
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    b.body(body).unwrap_or_default()
}

/// A URI path with its `%XX` escapes decoded.
pub(crate) fn percent_decode(s: &str) -> String {
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

type Job = Box<dyn FnOnce() + Send>;

/// A few worker threads for `art://` requests: a grid asks for hundreds
/// of thumbnails at once, and each may decode a big picture, so they wait
/// their turn instead of each getting a thread.
#[derive(Clone)]
pub struct ArtPool(std::sync::mpsc::Sender<Job>);

impl ArtPool {
    pub fn new() -> Self {
        let n = std::thread::available_parallelism().map_or(2, |n| n.get().clamp(2, 8));
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        let rx = Arc::new(std::sync::Mutex::new(rx));
        for _ in 0..n {
            let rx = rx.clone();
            std::thread::spawn(move || {
                loop {
                    // The lock is held only to take the next job.
                    let job = rx.lock().unwrap_or_else(|e| e.into_inner()).recv();
                    match job {
                        Ok(job) => job(),
                        Err(_) => break,
                    }
                }
            });
        }
        Self(tx)
    }

    pub fn run(&self, job: impl FnOnce() + Send + 'static) {
        // The workers live as long as the app, so this can't fail.
        let _ = self.0.send(Box::new(job));
    }
}
