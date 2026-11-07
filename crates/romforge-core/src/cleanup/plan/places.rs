//! Where a cleanup takes games from and puts them.

use std::path::{Path, PathBuf};

use super::super::Source;
use crate::Error;
use crate::home::Home;
use crate::settings::Settings;

/// The Source a cleanup reads (`from`, else the Source folder; with a
/// RomForge folder, everywhere outside it) and the folder it writes to.
pub(super) fn source_and_dest(
    s: &Settings,
    home: Option<&Home>,
    from: Option<&Path>,
    convert: bool,
) -> Result<(Source, PathBuf), Error> {
    Ok(match home {
        // Convert takes discs wherever they are.
        _ if convert => (Source::Folder(PathBuf::from("/")), PathBuf::new()),
        Some(h) => {
            let source = match from {
                Some(f) if h.contains(f) => {
                    return Err(Error::Settings(format!(
                        "{} is inside the RomForge folder; nothing to import from it",
                        f.display()
                    )));
                }
                Some(f) => Source::Folder(f.to_owned()),
                None => Source::Outside(h.root().to_owned()),
            };
            (source, h.games())
        }
        None => {
            let source = s
                .source_dir
                .clone()
                .ok_or_else(|| Error::Settings("set a Source folder first (Settings)".into()))?;
            let dest = s.library_dir.clone().unwrap_or_else(|| source.clone());
            (Source::Folder(from.map_or(source, Path::to_owned)), dest)
        }
    })
}
