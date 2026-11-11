//! Games by name for uQ's launcher (Spotlight): the rows of the library
//! list ([`crate::view::listing`]) that Play may start, matched against
//! what was typed, best first, at most [`MAX_HITS`].
//!
//! Everything from the bus is checked here before it is used: a query is
//! at most [`MAX_QUERY_BYTES`] with no control characters, and an id is
//! only ever a playable row's id from the current list.

use crate::view::{GameRow, LibraryView, search_title};

/// uQ shows at most 20 rows from a provider.
pub const MAX_HITS: usize = 20;
/// Longer queries get no rows.
pub const MAX_QUERY_BYTES: usize = 256;
/// uQ drops rows whose id is longer, so none longer is ever activated.
pub const MAX_ID_BYTES: usize = 512;

/// One launcher row.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Hit {
    /// The library row's id ([`GameRow::id`]), as Play takes it.
    pub id: String,
    /// The game's name without its tags ("The Legend of Zelda - A Link to
    /// the Past").
    pub title: String,
    /// Its platform, then what tells this version apart (regions,
    /// revision, translation, hack).
    pub subtitle: String,
    /// Its box art, when it has some.
    pub picture: Option<String>,
    /// 0 to 1.
    pub score: f64,
}

/// Lower case, letters and digits only, words split by single spaces.
fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(word.chars().flat_map(char::to_lowercase));
    }
    out
}

/// The query as it is matched, or none when it is too long, holds a
/// control character, or has no letter or digit.
pub fn query(raw: &str) -> Option<String> {
    if raw.len() > MAX_QUERY_BYTES || raw.chars().any(char::is_control) {
        return None;
    }
    Some(fold(raw)).filter(|q| !q.is_empty())
}

/// Whether `id` can be a row id at all; [`find`] still checks it is one.
pub fn id_shape(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_BYTES
        && !id.chars().any(char::is_control)
        && ["g:", "m:", "f:"].iter().any(|p| id.starts_with(p))
}

/// How well `name` (folded) matches `q` (folded), if it does.
fn score(name: &str, q: &str) -> Option<f64> {
    if name == q {
        return Some(1.0);
    }
    if name.starts_with(q) {
        return Some(0.9);
    }
    if name.contains(&format!(" {q}")) {
        return Some(0.8);
    }
    if name.contains(q) {
        return Some(0.65);
    }
    let words: Vec<&str> = name.split(' ').collect();
    q.split(' ')
        .all(|w| words.iter().any(|n| n.starts_with(w)))
        .then_some(0.55)
}

/// A row the launcher may offer: one Play can start, matched to a game.
fn offered(r: &GameRow) -> bool {
    r.playable && !r.packed && !r.unmatched
}

/// The name a row is shown and matched by.
fn title(r: &GameRow) -> String {
    let t = search_title(r.label.as_deref().unwrap_or(&r.title));
    if t.is_empty() { r.title.clone() } else { t }
}

fn hit(r: &GameRow, score: f64) -> Hit {
    let n = &r.name;
    let mut parts: Vec<String> = Vec::new();
    parts.extend(r.system_name.clone());
    if !n.regions.is_empty() {
        parts.push(n.regions.join(", "));
    }
    parts.extend(n.version.clone());
    parts.extend(n.status.clone());
    parts.extend(n.disc.as_ref().map(|d| format!("Disc {d}")));
    if n.translation.is_some() {
        parts.push("translation".into());
    }
    if n.hack.is_some() {
        parts.push("hack".into());
    }
    Hit {
        id: r.id.clone(),
        title: title(r),
        subtitle: parts.join(" · "),
        picture: r.pictures.get("boxart").cloned(),
        score,
    }
}

/// The best [`MAX_HITS`] games for `raw`: whole name, then its start, then
/// a word's start, then anywhere, then every word somewhere. A version
/// ranks just under its group's main row; ties go to the shorter name.
pub fn search(view: &LibraryView, raw: &str) -> Vec<Hit> {
    let Some(q) = query(raw) else {
        return Vec::new();
    };
    let mut found: Vec<(f64, usize, String, &GameRow)> = view
        .rows
        .iter()
        .filter(|r| offered(r))
        .filter_map(|r| {
            let name = fold(&title(r));
            let s = score(&name, &q)? - if r.main { 0.0 } else { 0.02 };
            Some((s, name.len(), name, r))
        })
        .collect();
    found.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then(a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.3.id.cmp(&b.3.id))
    });
    found
        .into_iter()
        .take(MAX_HITS)
        .map(|(s, _, _, r)| hit(r, s.clamp(0.0, 1.0)))
        .collect()
}

/// The game `id` names, if it is one the launcher may start.
pub fn find(view: &LibraryView, id: &str) -> Option<Hit> {
    if !id_shape(id) {
        return None;
    }
    view.rows
        .iter()
        .find(|r| r.id == id && offered(r))
        .map(|r| hit(r, 1.0))
}

#[cfg(test)]
mod tests;
