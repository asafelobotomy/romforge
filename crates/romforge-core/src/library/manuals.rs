//! A game's manual (milestone 26), by library row.

use std::path::{Path, PathBuf};

use crate::{Error, Library, manual, settings};

/// A row's system key, title and its files' SHA1s.
struct Game {
    system: String,
    title: String,
    sha1s: Vec<String>,
}

impl Library {
    fn manual_game(&self, row_id: &str) -> Result<Game, Error> {
        let v = self.cached_view()?;
        let row = v
            .rows
            .iter()
            .find(|r| r.id == row_id)
            .ok_or_else(|| Error::Launch(format!("no library row {row_id:?}")))?;
        let system = row
            .system
            .clone()
            .ok_or_else(|| Error::Launch(format!("{} has no platform", row.title)))?;
        let mut stmt = self
            .conn
            .prepare_cached("SELECT sha1 FROM files WHERE file_id = ?1 AND sha1 IS NOT NULL")?;
        let mut sha1s = Vec::new();
        for id in row.file_ids() {
            if let Ok(s) = stmt.query_row([id], |r| r.get::<_, String>(0)) {
                sha1s.push(s);
            }
        }
        Ok(Game {
            system,
            title: row.title.clone(),
            sha1s,
        })
    }

    /// Where manuals are kept: the RomForge folder's `Media/`, else the art
    /// cache.
    fn manual_media(&self) -> PathBuf {
        match &self.home {
            Some(h) => h.media(),
            None => crate::enrich::default_cache_dir().join(crate::layout::LOOSE_ART_DIR),
        }
    }

    pub fn manual_info(&self, row_id: &str) -> Result<manual::ManualInfo, Error> {
        let g = self.manual_game(row_id)?;
        manual::manual(&self.conn, &g.system, &g.title)
    }

    /// Ask Hasheous (once) for the game's Vimm's Lair manual id.
    pub fn manual_lookup(&self, row_id: &str) -> Result<Option<String>, Error> {
        let g = self.manual_game(row_id)?;
        manual::lookup_vimm(&self.conn, &g.system, &g.title, &g.sha1s)
    }

    /// Keep the user's PDF as the game's manual.
    pub fn add_manual(&mut self, row_id: &str, pdf: &Path) -> Result<PathBuf, Error> {
        let g = self.manual_game(row_id)?;
        manual::add_manual(
            &self.conn,
            &self.manual_media(),
            &g.system,
            &g.title,
            pdf,
            "added by you",
        )
    }

    /// Download the game's manual from Vimm's Lair (on the user's click).
    pub fn download_manual(&mut self, row_id: &str, dpi: u32) -> Result<PathBuf, Error> {
        let g = self.manual_game(row_id)?;
        let staging = match &self.home {
            Some(h) => h.staging(),
            None => settings::load(&self.conn)?.staging(),
        };
        manual::download_vimm(
            &self.conn,
            &self.manual_media(),
            &staging,
            &g.system,
            &g.title,
            dpi,
        )
    }

    pub fn remove_manual(&mut self, row_id: &str) -> Result<(), Error> {
        let g = self.manual_game(row_id)?;
        manual::remove_manual(&self.conn, &self.manual_media(), &g.system, &g.title)
    }

    pub fn set_manual_page(&mut self, row_id: &str, page: u32) -> Result<(), Error> {
        let g = self.manual_game(row_id)?;
        manual::set_page(&self.conn, &g.system, &g.title, page)
    }
}
