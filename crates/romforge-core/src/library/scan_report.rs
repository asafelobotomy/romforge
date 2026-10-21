//! What a scan reports.

use std::path::PathBuf;

use crate::view;

/// What Scan folder did: the scan, and whether the folder became the
/// Source (`Some(Err)`: it couldn't, and why).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FolderScan {
    pub report: ScanReport,
    pub became_source: Option<Result<(), String>>,
}

/// What a scan would find ([`Library::scan_preview`](crate::Library::scan_preview)).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ScanPreview {
    pub report: ScanReport,
    /// The games and files found in the folder, by status then title.
    pub games: Vec<PreviewGame>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PreviewGame {
    pub title: String,
    pub system: Option<String>,
    pub status: view::RowStatus,
    /// Not in the library before.
    pub new: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ScanReport {
    pub root: PathBuf,
    pub hashed: usize,
    pub bytes_hashed: u64,
    /// Files and archive members without a ROM extension (not hashed).
    pub skipped: usize,
    /// What those were, by extension, most first, so a format romforge
    /// doesn't know shows here. Left out: details files (`.romforge.md`)
    /// and the `romforge/` folder inside archives.
    pub skipped_kinds: Vec<SkippedKind>,
    /// Symlinked folders inside the tree (not followed).
    pub skipped_links: Vec<PathBuf>,
    /// Files (or archives) unchanged since the last scan, not read again.
    pub unchanged: usize,
    /// Files inside ScummVM games found before hashing (by ScummVM's
    /// list): the game is named by them, they aren't hashed one by one.
    pub in_games: usize,
    pub errors: Vec<(PathBuf, String, String)>,
    /// Loose `.cue`/`.gdi` sheets and the track files they name that are
    /// missing.
    pub sheet_problems: Vec<(PathBuf, String)>,
}

/// Files a scan skipped for their extension, by extension.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SkippedKind {
    /// Lower case, without the dot; empty for none.
    pub ext: String,
    pub count: usize,
    /// One of them (`path` or `path#member`).
    pub example: String,
}

impl SkippedKind {
    /// Several scans' kinds as one list, most first.
    pub fn merge<'a>(lists: impl IntoIterator<Item = &'a [SkippedKind]>) -> Vec<SkippedKind> {
        let mut by: std::collections::BTreeMap<String, SkippedKind> = Default::default();
        for k in lists.into_iter().flatten() {
            by.entry(k.ext.clone())
                .and_modify(|e| e.count += k.count)
                .or_insert_with(|| k.clone());
        }
        let mut v: Vec<SkippedKind> = by.into_values().collect();
        v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.ext.cmp(&b.ext)));
        v
    }

    /// `.txt 5, .nfo 3, no extension 2, and 4 more kinds`, at most `n`.
    pub fn summary(kinds: &[SkippedKind], n: usize) -> String {
        let mut parts: Vec<String> = kinds
            .iter()
            .take(n)
            .map(|k| {
                if k.ext.is_empty() {
                    format!("no extension {}", k.count)
                } else {
                    format!(".{} {}", k.ext, k.count)
                }
            })
            .collect();
        if kinds.len() > n {
            parts.push(format!("and {} more kinds", kinds.len() - n));
        }
        parts.join(", ")
    }
}

/// One source folder's rescan: its report, or the folder and why it failed.
pub type RefreshResult = Result<ScanReport, (PathBuf, String)>;

/// Running totals passed to the progress callback after every file or
/// member, and when the files to read are known and each is done.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct ScanProgress {
    pub hashed: usize,
    /// Bytes as read (an archive member's unpacked size).
    pub bytes_hashed: u64,
    pub skipped: usize,
    /// Files and archives to read (0 until the folder has been walked),
    /// and how many of them are done.
    pub units_total: usize,
    pub units_done: usize,
    /// Their size on disk, in all and done: this is what the bar follows.
    pub disk_total: u64,
    pub disk_done: u64,
    /// In a Refresh: which folder this is (from 0), of how many (0 for a
    /// single scan).
    pub folder: usize,
    pub folders: usize,
}

impl ScanProgress {
    /// As a job's step, in thousandths: each folder of a Refresh an equal
    /// share, filled by its on-disk bytes done.
    pub fn step(&self) -> crate::job::Step {
        let folders = self.folders.max(1) as u64;
        let within = match self.disk_total {
            0 if self.units_total > 0 => 1000 * self.units_done as u64 / self.units_total as u64,
            0 => 0,
            t => 1000 * self.disk_done.min(t) / t,
        };
        let mut detail = format!("{} of {} files", self.units_done, self.units_total);
        if self.folders > 1 {
            detail = format!("folder {} of {}: {detail}", self.folder + 1, self.folders);
        }
        crate::job::Step::new(1000 * self.folder as u64 + within, 1000 * folders, detail)
    }
}
