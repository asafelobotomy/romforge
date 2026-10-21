//! Applying patches, and finding a game by name.

use std::path::{Path, PathBuf};

use crate::{Error, Library, ScanOptions, Scope, patcher, settings, view};

impl Library {
    /// Where a patched game goes, and the scratch space for it: the
    /// RomForge folder's `Games/`, else the Library (or Source) folder.
    fn patch_places(&self) -> Result<(PathBuf, PathBuf), Error> {
        if let Some(h) = &self.home {
            return Ok((h.games(), h.staging()));
        }
        let s = settings::load(&self.conn)?;
        let dest = s.library_dir.clone().or(s.source_dir.clone()).ok_or_else(|| {
            Error::Settings("set a Library folder (or create the RomForge folder) first".into())
        })?;
        Ok((dest, s.staging()))
    }

    /// Work out applying a patch (a file you fetched) to a game: a new
    /// copy as a translation or hack of it (milestone 19). Nothing is
    /// written.
    pub fn patch_plan(
        &self,
        row_id: &str,
        patch: &Path,
        relation: Option<view::Relation>,
        tag: Option<&str>,
    ) -> Result<patcher::PatchPlan, Error> {
        let (games, staging) = self.patch_places()?;
        std::fs::create_dir_all(&staging)?;
        patcher::plan(&self.conn, &games, &staging, row_id, patch, relation, tag)
    }

    /// Apply it: the new game is written, named, journaled (`cleanup undo`
    /// removes it) and scanned.
    pub fn patch_apply(&mut self, plan: &patcher::PatchPlan) -> Result<patcher::Patched, Error> {
        let (_, staging) = self.patch_places()?;
        let done = patcher::apply(&mut self.conn, plan, &staging)?;
        self.refresh(&ScanOptions::default(), |_, _| {})?;
        // A folder outside the scanned ones (a Library not yet scanned).
        if let Some(dir) = done.target.parent()
            && !self.library_view()?.rows.iter().any(|r| r.title == done.title)
        {
            self.scan(dir, Scope::All)?;
        }
        Ok(done)
    }

    /// The one identified game whose title is `query` or contains it (any
    /// case), for commands that name a game.
    pub fn find_row(&self, query: &str) -> Result<view::GameRow, Error> {
        let rows = self.library_view()?.rows;
        if let Some(r) = rows.iter().find(|r| r.id == query || r.title == query) {
            return Ok(r.clone());
        }
        let q = query.to_lowercase();
        let found: Vec<&view::GameRow> = rows
            .iter()
            .filter(|r| r.status.identified() && r.title.to_lowercase().contains(&q))
            .collect();
        match found.as_slice() {
            [one] => Ok((*one).clone()),
            [] => Err(Error::Search(format!("no identified game like {query:?}"))),
            many => Err(Error::Search(format!(
                "{query:?} matches {} games: {}",
                many.len(),
                many.iter()
                    .take(5)
                    .map(|r| r.title.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            ))),
        }
    }

    /// The patches for a game: those a scan paired with it, then the others
    /// lying in its folders.
    pub fn patches_beside(&self, row_id: &str) -> Result<Vec<patcher::FoundPatch>, Error> {
        patcher::beside(&self.conn, row_id)
    }
}
