//! The search queue and naming games by hand. Each works on the list the
//! app shows ([`Library::library_view`], built once and kept), so any of its
//! row ids may be given.

use std::path::Path;

use crate::{Error, Library, enrich, search, view, wikidata};

impl Library {
    /// Games and files that need the owner (milestone 8).
    pub fn search_queue(&self, only: Option<&[String]>) -> Result<Vec<search::QueueItem>, Error> {
        search::queue(&self.conn, &*self.cached_view()?, only)
    }

    /// Search one scraper for a row (the Search dialog's results table).
    /// `api`: Wikidata's entity search ([`search::WIKIDATA_API`]).
    pub fn search_find(
        &self,
        row_id: &str,
        scraper: search::Scraper,
        text: Option<&str>,
        system: Option<&str>,
        api: &str,
    ) -> Result<Vec<search::Found>, Error> {
        search::find(
            &self.conn,
            &*self.cached_view()?,
            row_id,
            scraper,
            text,
            system,
            api,
        )
    }

    /// The preview pane for a result: details, DAT ROMs against your file,
    /// box art.
    pub fn search_preview(
        &self,
        row_id: &str,
        found: &search::Found,
        cache_dir: &Path,
        thumbnails_base: &str,
    ) -> Result<search::Preview, Error> {
        search::preview(
            &self.conn,
            &*self.cached_view()?,
            row_id,
            found,
            cache_dir,
            thumbnails_base,
        )
    }

    /// Take a result for a row, then fetch for that game what `opts` asks.
    pub fn search_choose(
        &mut self,
        row_id: &str,
        found: &search::Found,
        system: Option<&str>,
        opts: &enrich::EnrichOptions,
    ) -> Result<search::Chosen, Error> {
        let opts = self.with_extdb(opts);
        let v = self.cached_view()?;
        let taken = search::choose(&mut self.conn, &v, row_id, found, system, &opts)?;
        // The list built again once, and kept for the app's next look.
        let v = self.cached_view()?;
        Ok(search::Chosen {
            row_id: search::row_in(&v, &taken.system, &taken.title)?,
            enrich: taken.enrich,
        })
    }

    /// Name a row's files yourself ("identified by you").
    pub fn identify_as(&mut self, row_id: &str, system: &str, title: &str) -> Result<(), Error> {
        let v = self.cached_view()?;
        search::identify(&mut self.conn, &v, row_id, system, title, None)
    }

    /// Name a row's files as a translation or hack of a DAT game, listed
    /// under [`view::Derived::title`] ("translation of X", never verified).
    pub fn identify_derived(
        &mut self,
        row_id: &str,
        system: &str,
        derived: &view::Derived,
    ) -> Result<(), Error> {
        let v = self.cached_view()?;
        search::identify(
            &mut self.conn,
            &v,
            row_id,
            system,
            &derived.title(),
            Some(derived),
        )
    }

    pub fn forget_identity(&mut self, row_id: &str) -> Result<(), Error> {
        let v = self.cached_view()?;
        search::forget(&mut self.conn, &v, row_id)
    }

    pub fn wikidata_candidates(&self, row_id: &str) -> Result<Vec<wikidata::Candidate>, Error> {
        search::wikidata_candidates(&self.conn, &*self.cached_view()?, row_id)
    }

    pub fn settle_wikidata(&mut self, endpoint: &str, row_id: &str, qid: &str) -> Result<(), Error> {
        let v = self.cached_view()?;
        search::settle_wikidata(&mut self.conn, &v, endpoint, row_id, qid)
    }
}
