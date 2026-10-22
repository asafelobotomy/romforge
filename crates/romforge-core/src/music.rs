//! Stage 2, step 6: background music for the fullscreen mode, from a
//! folder of the user's own (Settings). romforge only reads it: the tracks
//! in it and one level of folders below, which the app plays (`music://`,
//! [`track`] keeping every path inside the folder).

use std::path::{Component, Path, PathBuf};

/// The kinds WebKitGTK plays (through GStreamer); a track it can't is
/// skipped.
const KINDS: &[(&str, &str)] = &[
    ("mp3", "audio/mpeg"),
    ("ogg", "audio/ogg"),
    ("oga", "audio/ogg"),
    ("opus", "audio/ogg"),
    ("flac", "audio/flac"),
    ("m4a", "audio/mp4"),
    ("wav", "audio/wav"),
];

/// The type of a track, or none for a file that isn't one.
pub fn kind(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    KINDS.iter().find(|(e, _)| *e == ext).map(|(_, m)| *m)
}

/// The tracks in `dir` (and its folders, one level down), by path, each
/// relative to `dir` with `/` between parts. Hidden files are left out.
pub fn tracks(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let entries = |d: &Path| -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(d)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .map(|e| e.path())
            .collect();
        v.sort();
        v
    };
    for p in entries(dir) {
        if p.is_dir() {
            for q in entries(&p) {
                if q.is_file() && kind(&q).is_some() {
                    out.extend(rel(dir, &q));
                }
            }
        } else if p.is_file() && kind(&p).is_some() {
            out.extend(rel(dir, &p));
        }
    }
    out
}

fn rel(dir: &Path, p: &Path) -> Option<String> {
    let r = p.strip_prefix(dir).ok()?;
    Some(
        r.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// The file for track `rel` in `dir`, and its type: only a track inside
/// `dir` (no `..`, no link out of it).
pub fn track(dir: &Path, rel: &str) -> Option<(PathBuf, &'static str)> {
    let r = Path::new(rel);
    if !r.components().all(|c| matches!(c, Component::Normal(_))) || r.components().count() > 2 {
        return None;
    }
    let kind = kind(r)?;
    let real = dir.join(r).canonicalize().ok()?;
    real.starts_with(dir.canonicalize().ok()?).then_some((real, kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_are_the_audio_files_and_stay_inside() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join("music");
        std::fs::create_dir_all(d.join("Album/deeper")).unwrap();
        for f in [
            "b.mp3",
            "a.ogg",
            "notes.txt",
            ".hidden.mp3",
            "Album/1.flac",
            "Album/deeper/x.mp3",
        ] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        std::fs::write(t.path().join("secret.mp3"), b"no").unwrap();
        assert_eq!(tracks(&d), ["Album/1.flac", "a.ogg", "b.mp3"]);
        assert_eq!(track(&d, "Album/1.flac").unwrap().1, "audio/flac");
        for bad in ["../secret.mp3", "/etc/passwd", "notes.txt", "Album/deeper/x.mp3"] {
            assert!(track(&d, bad).is_none(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(t.path().join("secret.mp3"), d.join("link.mp3")).unwrap();
            assert!(track(&d, "link.mp3").is_none(), "no links out");
        }
    }
}
