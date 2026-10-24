//! Milestone 10: cleanup & rename. Repacks each game into one file named by
//! its DAT name, with its art and details inside, under the guardrails of
//! house rule 7.
//!
//! - Cartridge games: `<dest>/<system>/<DAT name>.zip` holding the ROMs
//!   (DAT ROM names; your own file names for games you named) and a
//!   `romforge/` folder with the art and `<name>.romforge.md`.
//! - Disc games: `<dest>/<system>/<DAT name>.chd` (CD tracks, or a DVD
//!   image), with art and details in a sidecar folder of the same name (CHD
//!   can't hold extra files).
//!
//! `<dest>` is the Library folder, else the Source folder. ROM data is
//! streamed from the old file or archive straight into the new one; only a
//! 7z member (one compressed stream), or a big zip member packed with
//! anything but store or deflate, is unpacked into the staging folder.
//! Every new file is read back and its ROMs/tracks re-hashed against
//! the originals before anything else happens. Without "keep originals" the
//! originals then move to `.romforge-trash/<batch>/` next to them, and only
//! when every file in them went into this batch and no other game needs
//! them. Every step is journaled so [`undo`] (newest batch first) puts the
//! originals back and moves the new files to the trash. Nothing is ever
//! overwritten.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::Error;
use crate::home::Home;

/// Zip members up to this size are read into memory; bigger ones stream
/// from the zip (or, packed other than stored or deflated, are unpacked to
/// staging).
const IN_MEMORY: u64 = 64 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Zip,
    /// CD tracks (`.bin`) into a CD CHD.
    ChdCd,
    /// A DVD image (`.iso`) into a DVD CHD.
    ChdDvd,
    /// A game that is a folder (or an archive of one), every file of it
    /// into one zip (DOS).
    Folder,
    /// A game that is a folder, copied as a folder (ScummVM).
    Dir,
    /// One disk of a set, unzipped into the set's folder (floppy
    /// computers whose cores read a set's disks themselves).
    Loose,
    /// An arcade set: its zips (its own, its parent's, its BIOS's) and CHD
    /// folders, copied whole under their own names (milestone 17).
    Set,
}

/// One ROM of a game and where its data is now.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Part {
    /// Name in the new archive.
    pub name: String,
    pub path: PathBuf,
    /// Member of `path`, or empty for a plain file.
    pub member: String,
    pub size: u64,
    /// The data's SHA1 as it is now; the new file must hold the same bytes.
    pub sha1: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Item {
    pub row_id: String,
    pub title: String,
    pub system: String,
    pub format: Format,
    pub target: PathBuf,
    /// Folder for art and details (discs).
    pub sidecar: Option<PathBuf>,
    pub parts: Vec<Part>,
    /// Files on disk that go to the trash afterwards (without "keep
    /// originals"): every file in them is part of this batch.
    pub trash: Vec<PathBuf>,
    /// Files that stay because they hold more than this batch.
    pub kept: Vec<PathBuf>,
    /// Other copies of the same ROMs in the Source: redundant once the game
    /// is written, so trashed along with the originals.
    pub copies: Vec<Part>,
    /// A cue sheet that isn't the DAT's, beside the tracks (same archive or
    /// folder). Its pregaps go into the CHD, so it counts as taken; if it
    /// turns out not to name exactly the tracks, its file stays.
    pub sheet: Option<Part>,
    /// The game is in the RomForge folder already (at `target`): nothing
    /// is written, its `copies` outside only go to the trash.
    #[serde(default)]
    pub existing: bool,
    /// For a game that is a folder: the folder, or the archive holding it.
    /// Everything in it goes, and it goes to the trash whole.
    #[serde(default)]
    pub folder: Option<PathBuf>,
    /// For a ScummVM game: its game id, written as `<id>.scummvm` in the
    /// folder for RetroArch's core.
    #[serde(default)]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct CleanupPlan {
    pub dest: PathBuf,
    pub keep_originals: bool,
    /// Each game written in its own folder ("Replace originals", or
    /// Convert), not into `dest`.
    #[serde(default)]
    pub in_place: bool,
    pub items: Vec<Item>,
    /// (row title, why it isn't cleaned)
    pub skipped: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct CleanupReport {
    pub batch_id: i64,
    pub done: usize,
    pub trashed: usize,
    /// `.m3u` playlists written for complete multi-disk sets.
    #[serde(default)]
    pub playlists: usize,
    pub failed: Vec<(String, String)>,
    /// Asked to stop: the games after the last one done were left as
    /// they are.
    #[serde(default)]
    pub stopped: bool,
}

/// Which files a cleanup or import takes games from.
pub(crate) enum Source {
    /// The Source folder (no RomForge folder), or one folder being
    /// imported.
    Folder(PathBuf),
    /// Anywhere outside the RomForge folder.
    Outside(PathBuf),
}

impl Source {
    fn contains(&self, p: &Path) -> bool {
        match self {
            Source::Folder(dir) => p.starts_with(dir),
            Source::Outside(home) => !p.starts_with(home),
        }
    }
}

/// Work out what cleanup would do (nothing is touched). `only`: limit to
/// these library row ids. With a RomForge folder this is the import plan:
/// games outside it (or in `from` only) go to its `Games/`.
pub fn plan(
    conn: &Connection,
    only: Option<&[String]>,
    home: Option<&Home>,
    from: Option<&Path>,
) -> Result<CleanupPlan, Error> {
    plan_as(conn, only, home, from, How::Cleanup)
}

/// What a plan is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum How {
    /// Rename & Cleanup (or Import): into RomForge or the Library, or in
    /// place with "Replace originals".
    Cleanup,
    /// Convert: disc images into CHD beside themselves, under their own
    /// names, wherever they are.
    Convert,
}

/// Convert's plan: every identified disc game not yet a CHD (CD tracks
/// with their cue sheet, or one DVD image), written as a CHD in its own
/// folder under its own name; the originals as Rename & Cleanup's setting
/// says (kept, or to the trash). Discs CHD can't take (GameCube and Wii's
/// RVZ, odd track files) are listed as skipped.
pub fn convert_plan(
    conn: &Connection,
    only: Option<&[String]>,
    home: Option<&Home>,
) -> Result<CleanupPlan, Error> {
    plan_as(conn, only, home, None, How::Convert)
}

mod apply;
mod chd;
mod items;
mod parts;
mod plan;
#[cfg(test)]
mod tests;
mod trash;
mod undo;
mod unpack;
mod write;

pub(crate) use apply::journal_output;
pub use apply::{apply, disk_of};
pub(crate) use chd::write_chd;
pub(crate) use items::{archive_entries, safe_member_name};
pub use parts::details_md;
pub(crate) use parts::{named_parts, open_part, verified_parts};
pub(crate) use plan::TITLE_FOLDER_STAYS;
use plan::plan_as;
pub use trash::{TrashBatch, TrashSummary, empty_trash, trash_contents};
pub use undo::{Batch, CleanupUndone, batches, undo};
pub(crate) use write::{place, sha1_of, write_extras, zip_options};
