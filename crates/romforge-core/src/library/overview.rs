//! The read-only summary of the library (`overview`, `games`).

use std::collections::{BTreeSet, HashMap};

use super::lists::systems_with_files;
use crate::overview::{self, BiosGap, Games, GamesQuery, Overview};
use crate::{Error, Library, systems};

impl Library {
    /// The games matching `q`, as the app's list shows them.
    pub fn games_matching(&self, q: &GamesQuery) -> Result<Games, Error> {
        let view = self.cached_view()?;
        Ok(overview::games(&view, q))
    }

    /// Totals, platforms, recent additions, BIOS and lists still missing.
    pub fn overview(&self) -> Result<Overview, Error> {
        let view = self.cached_view()?;
        let mut newest: HashMap<i64, String> = HashMap::new();
        for row in self
            .conn
            .prepare("SELECT file_id, scanned_at FROM files")?
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        {
            let (id, at) = row?;
            newest.insert(id, at);
        }
        let files: i64 = self
            .conn
            .query_row("SELECT count(*) FROM files", [], |r| r.get(0))?;
        let bios = self
            .firmware_needs()
            .into_iter()
            .map(|n| BiosGap {
                system: n.system.to_owned(),
                name: systems::by_key(n.system).map_or_else(|| n.system.to_owned(), |s| s.name.to_owned()),
                emulator: n.emulator,
            })
            .collect();
        let with_dats: BTreeSet<String> = crate::report::systems_in_use(&self.conn)?
            .into_iter()
            .map(|s| s.key)
            .collect();
        let have = systems_with_files(&self.conn)?;
        let dats = self.summary()?.dats.len();
        let settings = self.settings()?;
        Ok(overview::overview(
            &view,
            &newest,
            files,
            [
                self.home.as_ref().map(|h| h.root().display().to_string()),
                settings.source_dir.map(|p| p.display().to_string()),
                settings.library_dir.map(|p| p.display().to_string()),
            ],
            bios,
            dats,
            overview::gaps(&have, &with_dats),
        ))
    }
}
