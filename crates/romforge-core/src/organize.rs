//! Milestone 2: rename and move identified files into a tidy tree, with a
//! dry run first, a journal of every move, and undo.
//!
//! Layout under the destination:
//! - an archive whose matched members all belong to one game:
//!   `<system>/<game>.<zip|7z>` (the archive itself is never rewritten);
//! - a loose file of a one-ROM game: `<system>/<rom name>`;
//! - loose files of a multi-ROM game (discs): `<system>/<game>/<rom name>`;
//!   a disc whose cue sheet differs from the DAT's moves only with its own
//!   sheet, which must list exactly its matched tracks, and keeps its names.
//!
//! Names come from the DAT; a file keeps its own extension. Only `matched`
//! files move. Nothing is ever overwritten. A move is a hard link plus
//! delete, a rename, or (across drives) a copy that is checked byte for byte
//! before the original is deleted.
//!
//! 1G1R ("one game, one ROM") keeps one version of each title: titles are
//! grouped by name without `(...)`/`[...]` tags, and the version with the
//! best region, then the highest revision, wins; betas, protos, demos and
//! the like lose to any release.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// What [`plan`] should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizeOptions {
    /// Only files under this folder move. Required: the library may hold
    /// scans of folders you never meant to reorganize.
    pub from: PathBuf,
    pub dest: PathBuf,
    /// Only files identified for this system key.
    pub system: Option<String>,
    /// Keep one version per title.
    pub one_g1r: bool,
    /// Region preference for 1G1R, best first.
    pub regions: Vec<String>,
}

pub const DEFAULT_REGIONS: &[&str] = &["USA", "World", "Europe", "Japan"];

impl OrganizeOptions {
    pub fn new(from: impl Into<PathBuf>, dest: impl Into<PathBuf>) -> Self {
        Self {
            from: from.into(),
            dest: dest.into(),
            system: None,
            one_g1r: false,
            regions: DEFAULT_REGIONS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Move {
    pub from: PathBuf,
    pub to: PathBuf,
    pub system: String,
    pub game: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Skip {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Plan {
    pub moves: Vec<Move>,
    pub skipped: Vec<Skip>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Applied {
    pub batch_id: i64,
    pub moved: usize,
    pub failed: Vec<(Move, String)>,
    /// Asked to stop: the moves after the last one done weren't made.
    #[serde(default)]
    pub stopped: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Undone {
    pub batch_id: i64,
    pub restored: usize,
    pub failed: Vec<(PathBuf, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Batch {
    pub batch_id: i64,
    pub created_at: String,
    pub dest: String,
    pub moves: i64,
    pub undone_at: Option<String>,
}

/// A game a file was identified as.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct GameRef {
    system: String,
    game: String,
}

struct Member {
    status: String,
    /// game -> (ROM name in the DAT, ROM count of the game, game has a cue)
    games: BTreeMap<GameRef, (String, i64, bool)>,
}

mod apply;
mod fs;
mod names;
mod plan;
#[cfg(test)]
mod tests;

pub use apply::{apply, batches, undo};
pub(crate) use fs::{copy_checked, move_file, remove_empty_parents, rename_noreplace, same_file, sync_dir};
pub use names::base_title;
pub(crate) use names::{CloneGroups, clone_groups, rank, sanitize};
pub use plan::plan;
