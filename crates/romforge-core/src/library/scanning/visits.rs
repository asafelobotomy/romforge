//! What a scan does with the files it doesn't hash.

use std::path::PathBuf;

use crate::{ScanReport, SkippedKind, layout, patch};

/// A file or member skipped for its extension: counted by kind (a format
/// romforge doesn't know shows up), and kept aside if it is a patch, to
/// pair with its game.
pub(super) fn skipped(
    report: &mut ScanReport,
    kinds: &mut std::collections::BTreeMap<String, SkippedKind>,
    patch_files: &mut std::collections::BTreeSet<PathBuf>,
    path: PathBuf,
    member: String,
) {
    report.skipped += 1;
    let name = if member.is_empty() {
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        member.clone()
    };
    if patch::is_patch_name(&name) && path.to_str().is_some() {
        patch_files.insert(path.clone());
    }
    if !layout::is_extra(&member) && !layout::is_extra(&name) {
        let ext = std::path::Path::new(&name)
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        kinds
            .entry(ext.clone())
            .and_modify(|k: &mut SkippedKind| k.count += 1)
            .or_insert_with(|| SkippedKind {
                ext,
                count: 1,
                example: if member.is_empty() {
                    path.display().to_string()
                } else {
                    format!("{}#{member}", path.display())
                },
            });
    }
}
