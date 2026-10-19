//! Enrich, art and platform icons.

use std::path::{Path, PathBuf};

use crate::{Error, Library, enrich, home, settings, systems};

/// The sidebar's icons: the style, and system key -> file.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct PlatformIcons {
    pub theme: String,
    pub icons: std::collections::BTreeMap<String, PathBuf>,
}

impl Library {
    /// Fetch metadata and art for the games you have (milestone 3).
    pub fn enrich(
        &mut self,
        opts: &enrich::EnrichOptions,
        progress: impl FnMut(enrich::EnrichEvent),
    ) -> Result<enrich::EnrichReport, Error> {
        let opts = self.with_extdb(opts);
        enrich::enrich(&mut self.conn, &opts, progress)
    }

    /// Enrich options using this library's downloaded databases unless the
    /// caller named a folder.
    pub(super) fn with_extdb(&self, opts: &enrich::EnrichOptions) -> enrich::EnrichOptions {
        let mut o = opts.clone();
        if o.extdb.is_none() {
            o.extdb = self.extdb_dir();
        }
        if o.art_dir.is_none() {
            o.art_dir = self.home.as_ref().map(home::Home::media);
        }
        o
    }

    /// Every picture of `kind` the sources have for a library row (a
    /// translation or hack looked up by its base game), with previews.
    /// Asks the network.
    pub fn art_choices(
        &self,
        opts: &enrich::EnrichOptions,
        row_id: &str,
        kind: enrich::ArtKind,
    ) -> Result<enrich::ArtChoices, Error> {
        let (system_id, title, lookup) = self.art_row(row_id)?;
        enrich::art_choices(
            &self.conn,
            &self.with_extdb(opts),
            system_id,
            &title,
            &lookup,
            kind,
            30,
        )
    }

    /// Make a picture [`Library::art_choices`] offered the row's `kind`, kept
    /// even when Enrich refreshes.
    pub fn choose_art(
        &mut self,
        opts: &enrich::EnrichOptions,
        row_id: &str,
        kind: enrich::ArtKind,
        url: &str,
        source: &str,
    ) -> Result<(), Error> {
        let (system_id, title, _) = self.art_row(row_id)?;
        enrich::choose_art(
            &self.conn,
            &self.with_extdb(opts),
            system_id,
            &title,
            kind,
            url,
            source,
        )
    }

    /// A row's platform, its title, and the title to look art up by.
    fn art_row(&self, row_id: &str) -> Result<(i64, String, String), Error> {
        let d = self.game_detail(row_id)?;
        let system_id = d
            .row
            .system
            .as_deref()
            .and_then(systems::by_key)
            .map(|s| s.id)
            .ok_or_else(|| Error::Settings(format!("{} has no platform to look art up for", d.row.title)))?;
        let lookup = d.derived.as_ref().map_or(d.row.title.clone(), |x| x.base.clone());
        Ok((system_id, d.row.title, lookup))
    }

    /// The sidebar's icons for the platforms in the library, in the style
    /// Settings picks (none: empty), fetched into `cache_dir` the first
    /// time from retroarch-assets (`base`, [`enrich::PLATFORM_ICONS_BASE`])
    /// or, for logos, Wikidata (`wikidata`, [`wikidata::ENDPOINT`]).
    /// Offline, only those fetched before.
    pub fn platform_icons(
        &self,
        cache_dir: &Path,
        base: &str,
        wikidata: &str,
    ) -> Result<PlatformIcons, Error> {
        let s = settings::load(&self.conn)?;
        let Some(theme) = s.icon_theme() else {
            return Ok(PlatformIcons::default());
        };
        let systems: Vec<&'static systems::System> = self
            .cached_view()?
            .platforms
            .iter()
            .filter_map(|p| p.system.as_deref().and_then(systems::by_key))
            .collect();
        Ok(PlatformIcons {
            theme: theme.to_owned(),
            icons: enrich::platform_icons(cache_dir, base, wikidata, theme, &systems).0,
        })
    }

    /// Metadata and cached art for games whose name contains `query`.
    pub fn game_info(&self, query: &str) -> Result<Vec<enrich::GameInfo>, Error> {
        enrich::info(&self.conn, query)
    }
}
