//! [`Library`](crate::Library)'s methods, by what they do.

mod art;
mod dat_import;
mod dat_lists;
mod dats;
mod edit;
mod emulators;
mod files;
mod folders;
mod manuals;
mod open;
mod patching;
mod scan_report;
mod scanning;
mod scummvm_io;
mod searching;
mod view_cache;

pub use art::PlatformIcons;
pub use dat_lists::{DatState, DatStatus};
pub use dats::ImportedDat;
pub use scan_report::{
    FolderScan, PreviewGame, RefreshResult, ScanPreview, ScanProgress, ScanReport, SkippedKind,
};
pub use scummvm_io::ScummvmList;
