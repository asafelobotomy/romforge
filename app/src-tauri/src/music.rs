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
    let respond = |status: u16, more: &[(&str, &str)], body: Vec<u8>| {
        let mut headers = vec![
            ("Accept-Ranges", "bytes"),
            ("Access-Control-Allow-Origin", "*"),
            (
                "Access-Control-Expose-Headers",
                "Content-Range, Content-Length, Accept-Ranges",
            ),
            ("Cache-Control", "no-store"),
        ];
        headers.extend_from_slice(more);
        crate::art::response(status, &headers, body)
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
    let asked = request
        .headers()
        .get("range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| range(v, len));
    let (start, end, status) = match asked {
        // At most 8 MB a ranged response: the player asks again for more.
        Some(Some((s, e))) => (s, e.min(s + (8 << 20) - 1), 206),
        Some(None) => {
            let all = format!("bytes */{len}");
            return respond(416, &[("Content-Range", &all)], Vec::new());
        }
        None => (0, len.saturating_sub(1), 200),
    };
    let mut body = vec![0; if len == 0 { 0 } else { (end + 1 - start) as usize }];
    if len > 0 && (file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut body).is_err()) {
        return not_found();
    }
    let (size, at) = (body.len().to_string(), format!("bytes {start}-{end}/{len}"));
    let mut headers = vec![("Content-Type", kind), ("Content-Length", size.as_str())];
    if status == 206 {
        headers.push(("Content-Range", at.as_str()));
    }
    respond(status, &headers, body)
}

/// `Range: bytes=a-b`, `a-` or `-n` (one range; the player asks for no
/// more) of a `len`-byte file: `None` when it isn't one to read (the whole
/// file is sent), `Some(None)` when it asks for bytes past the end.
fn range(header: &str, len: u64) -> Option<Option<(u64, u64)>> {
    let (a, b) = header.trim().strip_prefix("bytes=")?.split_once('-')?;
    let (a, b) = (a.trim(), b.trim());
    if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        return Some((n > 0 && len > 0).then(|| (len.saturating_sub(n), len - 1)));
    }
    let start: u64 = a.parse().ok()?;
    let end = if b.is_empty() { u64::MAX } else { b.parse().ok()? };
    if start > end {
        return None;
    }
    Some((start < len).then(|| (start, end.min(len - 1))))
}

#[cfg(test)]
mod tests {
    use super::range;

    #[test]
    fn ranges() {
        assert_eq!(range("bytes=0-9", 100), Some(Some((0, 9))));
        assert_eq!(range("bytes=90-", 100), Some(Some((90, 99))));
        assert_eq!(range("bytes=90-500", 100), Some(Some((90, 99))));
        assert_eq!(range("bytes=-10", 100), Some(Some((90, 99))));
        assert_eq!(range("bytes=100-", 100), Some(None), "past the end: 416");
        assert_eq!(range("bytes=5-1", 100), None, "not a range: the whole file");
        assert_eq!(range("items=0-1", 100), None);
    }
}
