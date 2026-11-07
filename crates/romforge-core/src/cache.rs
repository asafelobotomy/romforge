//! Keeping the cache folder from growing for ever. What is in it can always
//! be made or fetched again; the art Enrich keeps is elsewhere (`art/`,
//! and the RomForge folder's `Media/`) and is never touched here.

use std::path::Path;
use std::time::{Duration, SystemTime};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// Folders of the cache made again on demand, and how long a file in one
/// is kept after it was last written.
const MADE_AGAIN: &[(&str, Duration)] = &[
    // The art picker's previews: only for the picker that fetched them.
    (crate::enrich::CHOICES_DIR, DAY),
    // A game's box shown before it is enriched.
    ("preview", Duration::from_secs(30 * 24 * 60 * 60)),
    // Thumbnails are named by their picture's path and time, so a changed
    // picture leaves its old one behind.
    ("thumbs", Duration::from_secs(180 * 24 * 60 * 60)),
];

/// Remove what has outlived its time in `cache`'s made-again folders.
/// Returns how many files went. Errors are skipped: a file in use stays.
pub fn prune(cache: &Path) -> usize {
    prune_at(cache, SystemTime::now())
}

fn prune_at(cache: &Path, now: SystemTime) -> usize {
    let mut gone = 0;
    for (folder, keep) in MADE_AGAIN {
        let dir = cache.join(folder);
        for e in walkdir::WalkDir::new(&dir)
            .min_depth(1)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let old = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| now.duration_since(t).ok())
                .is_some_and(|age| age > *keep);
            if old && std::fs::remove_file(e.path()).is_ok() {
                gone += 1;
            }
        }
    }
    gone
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_of_made_again_folders_go_and_nothing_else() {
        let t = tempfile::tempdir().unwrap();
        let c = t.path();
        for f in ["art-choices/a.png", "thumbs/x/b.png", "art/SNES/keep.png"] {
            std::fs::create_dir_all(c.join(f).parent().unwrap()).unwrap();
            std::fs::write(c.join(f), b"x").unwrap();
        }
        assert_eq!(prune_at(c, SystemTime::now()), 0, "all new");
        let later = SystemTime::now() + 2 * DAY;
        assert_eq!(prune_at(c, later), 1, "the picker's preview only");
        assert!(!c.join("art-choices/a.png").exists());
        assert!(c.join("thumbs/x/b.png").exists());
        assert!(c.join("art/SNES/keep.png").exists(), "kept art is never touched");
    }
}
