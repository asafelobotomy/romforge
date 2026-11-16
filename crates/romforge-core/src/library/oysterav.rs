//! What `romforge-cli oysterav` can look at: the things romforge keeps.

use std::path::PathBuf;

use crate::{Error, Library};

/// A place oysterAV can be asked to look at.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AvTarget {
    /// `emulators`, `games`, or the path itself.
    pub name: String,
    pub path: PathBuf,
}

impl Library {
    /// `emulators` (the RomForge folder's `Emulators/`: AppImages and
    /// cores) and `games` (its `Games/` and the Library folder, where they
    /// exist). Only places that are there.
    pub fn oysterav_targets(&self) -> Result<Vec<AvTarget>, Error> {
        let mut out = Vec::new();
        if let Some(h) = &self.home {
            out.push(AvTarget {
                name: "emulators".into(),
                path: h.emulators(),
            });
            out.push(AvTarget {
                name: "games".into(),
                path: h.games(),
            });
        }
        if let Some(dir) = crate::settings::load(&self.conn)?.library_dir
            && !out.iter().any(|t| t.path == dir)
        {
            out.push(AvTarget {
                name: "games".into(),
                path: dir,
            });
        }
        out.retain(|t| t.path.is_dir());
        Ok(out)
    }
}
