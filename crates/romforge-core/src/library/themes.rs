//! The fullscreen mode's themes for this library ([`crate::themes`]): the
//! user's in `RomForge/Themes/` (or `themes/` beside the database), and
//! the pick of theme and its options, kept in the library's settings.

use std::path::PathBuf;

use crate::themes::{self, Applied, Entry, Selection};
use crate::{Error, Library};

impl Library {
    /// Where the user's themes are (made when asked to open it); none for
    /// a library in memory.
    pub fn themes_dir(&self) -> Option<PathBuf> {
        match &self.home {
            Some(h) => Some(h.themes()),
            None => self.path.as_deref().map(|p| {
                p.parent()
                    .unwrap_or(std::path::Path::new("."))
                    .join(crate::layout::LOOSE_THEMES_DIR)
            }),
        }
    }

    pub fn themes(&self) -> Vec<Entry> {
        themes::list(self.themes_dir().as_deref())
    }

    /// The pick, as it fits the themes there are.
    pub fn theme_selection(&self) -> Result<Selection, Error> {
        let s = themes::load(&self.conn)?;
        Ok(themes::fit(self.themes_dir().as_deref(), &s).0)
    }

    /// Keep `s` (made to fit first) and return what it applies.
    pub fn set_theme(&self, s: &Selection) -> Result<Applied, Error> {
        let dir = self.themes_dir();
        let (fitted, _) = themes::fit(dir.as_deref(), s);
        themes::save(&self.conn, &fitted)?;
        Ok(themes::applied(dir.as_deref(), &fitted))
    }

    /// What the fullscreen mode applies now.
    pub fn theme_applied(&self) -> Result<Applied, Error> {
        let s = themes::load(&self.conn)?;
        Ok(themes::applied(self.themes_dir().as_deref(), &s))
    }
}
