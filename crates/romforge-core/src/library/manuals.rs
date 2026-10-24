//! A game's manual (milestone 26), by library row.

use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;

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

    fn manual_staging(&self) -> Result<PathBuf, Error> {
        Ok(match &self.home {
            Some(h) => h.staging(),
            None => settings::load(&self.conn)?.staging(),
        })
    }

    /// The manuals archive.org has for the game, best first.
    pub fn manual_offers(&self, row_id: &str) -> Result<Vec<manual::ManualOffer>, Error> {
        let g = self.manual_game(row_id)?;
        // The game's DAT parent (one name: a game's name is unique in its
        // system's DATs).
        let parents: Vec<String> = self
            .conn
            .prepare_cached(
                "SELECT DISTINCT g.cloneof FROM games g JOIN dats d ON d.dat_id = g.dat_id
                 JOIN systems s ON s.system_id = d.system_id
                 WHERE s.key = ?1 AND g.name = ?2 AND g.cloneof IS NOT NULL",
            )?
            .query_map([&g.system, &g.title], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let parent = (parents.len() == 1).then(|| parents[0].as_str());
        let mut out = manual::find_manuals(&crate::enrich::default_cache_dir(), &g.system, &g.title, parent)?;
        // ScreenScraper's, seen by Enrich (it needs the keys, put back
        // only to download).
        let ss: Option<String> = self
            .conn
            .query_row(
                "SELECT m.value FROM game_meta m JOIN systems s ON s.system_id = m.system_id
                 WHERE s.key = ?1 AND m.title = ?2 AND m.field = 'manual_screenscraper'",
                [&g.system, &g.title],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(url) = ss {
            out.push(manual::ManualOffer {
                source: "ScreenScraper".into(),
                name: format!("{}.pdf", g.title),
                langs: String::new(),
                url,
                size: None,
            });
        }
        // An arcade set's from the Arcade Database, asked now (one call).
        if crate::systems::by_key(&g.system).is_some_and(|s| s.arcade)
            && let Ok(Some(url)) =
                crate::artsources::adb_manual_url(&crate::artsources::Bases::default(), &g.title)
        {
            out.push(manual::ManualOffer {
                source: "Arcade Database".into(),
                name: format!("{}.pdf", g.title),
                langs: String::new(),
                url,
                size: None,
            });
        }
        Ok(out)
    }

    /// Download the manual the user picked from [`Self::manual_offers`].
    pub fn download_manual_offer(
        &mut self,
        row_id: &str,
        offer: &manual::ManualOffer,
    ) -> Result<PathBuf, Error> {
        let g = self.manual_game(row_id)?;
        let staging = self.manual_staging()?;
        manual::download_offer(
            &self.conn,
            &self.manual_media(),
            &staging,
            &g.system,
            &g.title,
            offer,
            &settings::load(&self.conn)?.keys,
        )
    }

    /// Download the game's manual from Vimm's Lair (on the user's click).
    pub fn download_manual(&mut self, row_id: &str, dpi: u32) -> Result<PathBuf, Error> {
        let g = self.manual_game(row_id)?;
        let staging = self.manual_staging()?;
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

    /// The highlights and notes in the game's manual.
    pub fn manual_marks(&self, row_id: &str) -> Result<Vec<manual::Mark>, Error> {
        let g = self.manual_game(row_id)?;
        manual::marks(&self.conn, &g.system, &g.title)
    }

    pub fn add_manual_mark(&mut self, row_id: &str, mark: &manual::NewMark) -> Result<i64, Error> {
        let g = self.manual_game(row_id)?;
        manual::add_mark(&self.conn, &g.system, &g.title, mark)
    }

    pub fn edit_manual_mark(&mut self, id: i64, note: &str, color: &str) -> Result<(), Error> {
        manual::edit_mark(&self.conn, id, note, color)
    }

    pub fn remove_manual_mark(&mut self, id: i64) -> Result<(), Error> {
        manual::remove_mark(&self.conn, id)
    }

    /// The words recognised on a page of the game's manual, if it has been.
    pub fn manual_ocr(
        &self,
        row_id: &str,
        page: u32,
        lang: &str,
    ) -> Result<Option<Vec<manual::OcrWord>>, Error> {
        let g = self.manual_game(row_id)?;
        manual::ocr_page(&self.conn, &g.system, &g.title, page, lang)
    }

    pub fn save_manual_ocr(
        &mut self,
        row_id: &str,
        page: u32,
        lang: &str,
        words: &[manual::OcrWord],
    ) -> Result<(), Error> {
        let g = self.manual_game(row_id)?;
        manual::save_ocr(&self.conn, &g.system, &g.title, page, lang, words)
    }

    /// The recognised text of every page done so far (for search).
    pub fn manual_ocr_texts(&self, row_id: &str, lang: &str) -> Result<Vec<(u32, String)>, Error> {
        let g = self.manual_game(row_id)?;
        manual::ocr_texts(&self.conn, &g.system, &g.title, lang)
    }

    pub fn set_manual_page(&mut self, row_id: &str, page: u32) -> Result<(), Error> {
        let g = self.manual_game(row_id)?;
        manual::set_page(&self.conn, &g.system, &g.title, page)
    }
}
