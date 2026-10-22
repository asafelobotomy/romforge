//! Enrich's options from what a shell was asked (the app's chips, the
//! CLI's flags), built one way for both.

use super::{ArtKind, EnrichOptions, default_cache_dir};
use crate::Error;

impl EnrichOptions {
    /// For `system` (`None`: every platform): the `art` kinds named
    /// (`None`: every kind; `none` is skipped, an unknown name refused),
    /// Wikidata or not, `refresh` to fetch again what is there; saved
    /// under the default cache.
    pub fn chosen(
        system: Option<String>,
        art: Option<&[String]>,
        wikidata: bool,
        refresh: bool,
    ) -> Result<Self, Error> {
        let art = match art {
            None => ArtKind::ALL.to_vec(),
            Some(names) => names
                .iter()
                .filter(|a| !a.eq_ignore_ascii_case("none"))
                .map(|a| {
                    ArtKind::parse(a).ok_or_else(|| {
                        let kinds: Vec<&str> = ArtKind::ALL.iter().map(|k| k.as_str()).collect();
                        Error::Settings(format!("unknown art kind {a:?} ({}, none)", kinds.join(", ")))
                    })
                })
                .collect::<Result<_, _>>()?,
        };
        Ok(Self {
            system,
            art,
            refresh,
            wikidata: wikidata.then(|| crate::wikidata::ENDPOINT.to_owned()),
            ..Self::new(default_cache_dir())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_kinds_and_refuses_unknown_ones() {
        let o = EnrichOptions::chosen(None, None, false, true).unwrap();
        assert_eq!(o.art, ArtKind::ALL.to_vec());
        assert!(o.wikidata.is_none() && o.refresh);
        let names = ["none".to_owned(), "boxart".to_owned()];
        let o = EnrichOptions::chosen(Some("snes".into()), Some(&names), true, false).unwrap();
        assert_eq!(o.art, vec![ArtKind::Boxart]);
        assert!(o.wikidata.is_some());
        assert!(EnrichOptions::chosen(None, Some(&["poster".to_owned()]), true, false).is_err());
    }
}
