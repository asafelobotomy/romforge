//! Background music (stage 2, step 6): `music://localhost/<track>` serves
//! one track of the music folder Settings names, nothing outside it
//! (`romforge_core::music::track`). The page fetches it whole and plays it
//! as a Blob: WebKitGTK's media player drops a custom scheme's response
//! after its first range (tried with 2.52). Ranges are still answered.

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::RwLock;

use crate::db::{Db, Res, read};

/// The music folder, for the scheme (it never waits on the database).
static DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

pub fn set_dir(dir: Option<PathBuf>) {
    *DIR.write().unwrap_or_else(|e| e.into_inner()) = dir;
}

/// The music folder's tracks (after Settings changes, it is read again).
#[tauri::command]
pub async fn music_tracks(db: tauri::State<'_, Db>) -> Res<Vec<String>> {
    let dir = read(&db, |lib| Ok(lib.settings()?.music_dir)).await?;
    set_dir(dir.clone());
    Ok(dir.map(|d| romforge_core::music::tracks(&d)).unwrap_or_default())
}

pub fn serve(request: &tauri::http::Request<Vec<u8>>) -> tauri::http::Response<Vec<u8>> {
    let respond = |status: u16, headers: &[(&str, String)], body: Vec<u8>| {
        let mut b = tauri::http::Response::builder()
            .status(status)
            .header("Accept-Ranges", "bytes")
            .header("Access-Control-Allow-Origin", "*")
            .header(
                "Access-Control-Expose-Headers",
                "Content-Range, Content-Length, Accept-Ranges",
            )
            .header("Cache-Control", "no-store");
        for (k, v) in headers {
            b = b.header(*k, v);
        }
        b.body(body).unwrap_or_default()
    };
    let not_found = || respond(404, &[], Vec::new());
    let Some(dir) = DIR.read().unwrap_or_else(|e| e.into_inner()).clone() else {
        return not_found();
    };
    let rel = crate::art::percent_decode(request.uri().path().trim_start_matches('/'));
    let Some((path, kind)) = romforge_core::music::track(&dir, &rel) else {
        return not_found();
    };
    let Ok(mut file) = std::fs::File::open(&path) else {
        return not_found();
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    // `Range: bytes=a-b` (one range; the player asks for no more).
    let range = request
        .headers()
        .get("range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes="))
        .and_then(|v| v.split_once('-'))
        .and_then(|(a, b)| {
            let start: u64 = a.parse().ok()?;
            let end = if b.is_empty() {
                len.saturating_sub(1)
            } else {
                b.parse().ok()?
            };
            (start <= end && end < len).then_some((start, end))
        });
    let (start, end, status) = match range {
        // At most 8 MB a ranged response: the player asks again for more.
        Some((s, e)) => (s, e.min(s + (8 << 20) - 1), 206),
        None => (0, len.saturating_sub(1), 200),
    };
    let mut body = vec![0; if len == 0 { 0 } else { (end + 1 - start) as usize }];
    if len > 0 && (file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut body).is_err()) {
        return not_found();
    }
    let mut headers = vec![
        ("Content-Type", kind.to_owned()),
        ("Content-Length", body.len().to_string()),
    ];
    if status == 206 {
        headers.push(("Content-Range", format!("bytes {start}-{end}/{len}")));
    }
    respond(status, &headers, body)
}
