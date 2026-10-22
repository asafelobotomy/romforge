//! A list's rows as a tree: each game's original, then its versions
//! under their headings (the CLI's `games`; the app's list does the same).

use super::{Category, GameRow, LibraryView};

impl LibraryView {
    /// The games' originals (rows with [`GameRow::main`]) on `platform`
    /// (a system key, any case; `None`: every platform), in list order.
    pub fn main_rows<'a>(&'a self, platform: Option<&'a str>) -> impl Iterator<Item = &'a GameRow> + 'a {
        self.rows.iter().filter(move |r| {
            r.main && platform.is_none_or(|p| r.system.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(p)))
        })
    }

    /// The versions grouped with `row`, by heading in [`Category::ORDER`]
    /// (headings with none left out).
    pub fn versions_of(&self, row: &GameRow) -> Vec<(Category, Vec<&GameRow>)> {
        let Some(group) = &row.group else {
            return Vec::new();
        };
        Category::ORDER
            .into_iter()
            .map(|c| {
                let of = self
                    .rows
                    .iter()
                    .filter(|m| m.group.as_ref() == Some(group) && m.category == Some(c))
                    .collect::<Vec<_>>();
                (c, of)
            })
            .filter(|(_, of)| !of.is_empty())
            .collect()
    }
}
