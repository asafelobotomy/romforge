//! The language descriptions are fetched in (Wikipedia's article,
//! ScreenScraper's synopsis): one of [`LANGUAGES`], else the system's
//! (`$LANG`), else English.

/// The languages offered, as (ISO 639-1 code, name).
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("es", "Español"),
    ("it", "Italiano"),
    ("pt", "Português"),
    ("nl", "Nederlands"),
    ("sv", "Svenska"),
    ("pl", "Polski"),
    ("ru", "Русский"),
    ("ja", "日本語"),
];

fn known(code: &str) -> Option<&'static str> {
    LANGUAGES
        .iter()
        .map(|(c, _)| *c)
        .find(|c| c.eq_ignore_ascii_case(code))
}

/// The code a `$LANG`-style value names (`fr_FR.UTF-8`: fr), if offered.
pub fn from_locale(locale: &str) -> Option<&'static str> {
    known(locale.split(['_', '.', '@', '-']).next().unwrap_or(""))
}

impl super::Settings {
    /// The language to fetch descriptions in: the one set, else the
    /// system's, else English.
    pub fn language(&self) -> &'static str {
        known(self.language.trim())
            .or_else(|| {
                ["LC_ALL", "LC_MESSAGES", "LANG"]
                    .iter()
                    .find_map(|v| from_locale(&std::env::var(v).ok()?))
            })
            .unwrap_or("en")
    }
}

/// A language setting as saved: an offered code, or empty for the system's.
pub(super) fn cleaned(code: &str) -> Result<String, String> {
    let c = code.trim();
    if c.is_empty() {
        return Ok(String::new());
    }
    known(c).map(str::to_owned).ok_or_else(|| {
        format!(
            "no description language {c:?} (one of {})",
            LANGUAGES.iter().map(|(c, _)| *c).collect::<Vec<_>>().join(", ")
        )
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_locale_names_its_language() {
        assert_eq!(super::from_locale("fr_FR.UTF-8"), Some("fr"));
        assert_eq!(super::from_locale("en"), Some("en"));
        assert_eq!(super::from_locale("C"), None);
        assert!(super::cleaned("xx").is_err());
        assert_eq!(super::cleaned(" DE ").unwrap(), "de");
    }
}
