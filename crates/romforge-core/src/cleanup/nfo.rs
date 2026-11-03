//! A game's details as a `.nfo` (XML), beside its `.romforge.md` in the
//! `romforge/` folder, for the media managers that read one (Kodi, tinyMediaManager,
//! Emulation Station's scrapers). The layout is romforge's own and small:
//! only what romforge holds, nothing invented.

use std::collections::BTreeMap;

/// (element, the field it comes from).
const ELEMENTS: [(&str, &str); 10] = [
    ("year", "releaseyear"),
    ("developer", "developer"),
    ("publisher", "publisher"),
    ("genre", "genre"),
    ("players", "users"),
    ("country", "country"),
    ("series", "series"),
    ("franchise", "franchise"),
    ("serial", "serial"),
    ("plot", "description"),
];

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

/// The `.nfo` for a game: its title, platform and the fields romforge has.
pub fn nfo(title: &str, platform: &str, fields: &BTreeMap<String, String>) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<game>\n");
    s += &format!("  <title>{}</title>\n", escape(title));
    s += &format!("  <platform>{}</platform>\n", escape(platform));
    for (element, key) in ELEMENTS {
        if let Some(v) = fields.get(key).filter(|v| !v.trim().is_empty()) {
            s += &format!("  <{element}>{}</{element}>\n", escape(v));
        }
    }
    s += "</game>\n";
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_what_is_known_and_escapes_it() {
        let mut f = BTreeMap::new();
        f.insert("developer".to_owned(), "Q&A <Co>".to_owned());
        f.insert("genre".to_owned(), String::new());
        let x = nfo("Game \"One\"", "SNES", &f);
        assert!(x.contains("<title>Game &quot;One&quot;</title>"));
        assert!(x.contains("<developer>Q&amp;A &lt;Co&gt;</developer>"));
        assert!(!x.contains("<genre>"), "an empty field is left out");
        assert!(x.starts_with("<?xml") && x.ends_with("</game>\n"));
    }
}
