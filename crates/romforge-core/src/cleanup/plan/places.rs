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

/// A file stem without the ` (Track n)` a Redump track file ends in, so a
/// disc converted from it is named after the game. Unchanged when nothing
/// else would be left.
pub(super) fn without_track_suffix(stem: &str) -> &str {
    let Some(head) = stem.strip_suffix(')') else {
        return stem;
    };
    let Some((name, n)) = head.rsplit_once(" (Track ") else {
        return stem;
    };
    if name.trim().is_empty() || n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return stem;
    }
    name
}

#[cfg(test)]
mod tests {
    use super::without_track_suffix as w;

    #[test]
    fn track_suffix_is_stripped_only_when_it_is_one() {
        assert_eq!(w("Game (USA) (Track 1)"), "Game (USA)");
        assert_eq!(w("Game (Track 12)"), "Game");
        assert_eq!(w("Game (USA)"), "Game (USA)");
        assert_eq!(w("(Track 1)"), "(Track 1)");
        assert_eq!(w("Game (Track x)"), "Game (Track x)");
    }
}
