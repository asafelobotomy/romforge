//! Sorting rows, and the title people search for.

use super::GameRow;

/// What a list of games can be sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Title,
    Platform,
    Year,
    Status,
}

impl SortBy {
    pub fn parse(s: &str) -> Option<SortBy> {
        match s.to_ascii_lowercase().as_str() {
            "title" | "name" => Some(SortBy::Title),
            "platform" | "system" => Some(SortBy::Platform),
            "year" => Some(SortBy::Year),
            "status" => Some(SortBy::Status),
            _ => None,
        }
    }
}

/// Sort rows by `by` (then title), ascending or not; rows without a year
/// or platform go last either way. Stable, so a group's versions keep their
/// order among equals.
pub fn sort_rows(rows: &mut [GameRow], by: SortBy, descending: bool) {
    let title = |r: &GameRow| sort_key(r.label.as_deref().unwrap_or(&r.title));
    rows.sort_by(|a, b| {
        let (missing, ord) = match by {
            SortBy::Title => (std::cmp::Ordering::Equal, title(a).cmp(&title(b))),
            SortBy::Platform => (
                a.system_name.is_none().cmp(&b.system_name.is_none()),
                a.system_name.cmp(&b.system_name),
            ),
            SortBy::Year => (a.year.is_none().cmp(&b.year.is_none()), a.year.cmp(&b.year)),
            SortBy::Status => (std::cmp::Ordering::Equal, a.status.cmp(&b.status)),
        };
        let ord = if descending { ord.reverse() } else { ord };
        missing.then(ord).then_with(|| title(a).cmp(&title(b)))
    });
}

/// Sort "Legend of Zelda, The - ..." under L, ignoring case.
pub(super) fn sort_key(title: &str) -> String {
    title.to_lowercase()
}

/// The title people search for: tags removed, "Zelda, The" as "The Zelda".
pub fn search_title(name: &str) -> String {
    let mut out = String::new();
    let mut depth = 0i32;
    for c in name.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let t = out.split_whitespace().collect::<Vec<_>>().join(" ");
    // "Legend of Zelda, The - A Link to the Past" -> "The Legend of Zelda - A Link to the Past"
    let (head, tail) = match t.split_once(" - ") {
        Some((h, rest)) => (h.to_owned(), format!(" - {rest}")),
        None => (t.clone(), String::new()),
    };
    for article in ["The", "A", "An"] {
        if let Some(stem) = head.strip_suffix(&format!(", {article}")) {
            return format!("{article} {stem}{tail}");
        }
    }
    t
}
