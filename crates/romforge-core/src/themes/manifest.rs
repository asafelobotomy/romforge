//! A theme's `theme.json`, and what makes one refused.

use std::collections::BTreeMap;

/// The theme API this romforge speaks ([`super`]'s hooks and variables).
pub const API: u32 = 1;

/// The fullscreen mode's layouts a theme can pick from.
pub const LAYOUTS: &[&str] = &["grid", "list"];

/// What a card without art shows.
pub const FALLBACKS: &[&str] = &["title", "platform"];

/// `theme.json`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub api: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub licence: String,
    /// Where the licence is read (a community theme's licence is shown
    /// before it is downloaded).
    #[serde(default)]
    pub licence_url: String,
    /// The theme's own version, for updates from the community index.
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// The layout it starts in.
    #[serde(default = "grid")]
    pub layout: String,
    /// The layouts it allows (none: only `layout`).
    #[serde(default)]
    pub layouts: Vec<String>,
    #[serde(default = "title")]
    pub fallback: String,
    /// Colour schemes: name → variables. "light" and "dark" together can
    /// follow the system's.
    pub schemes: BTreeMap<String, Vars>,
    #[serde(default)]
    pub options: Vec<ThemeOption>,
}

/// CSS custom properties, each `--fs-…`.
pub type Vars = BTreeMap<String, String>;

/// A choice the theme offers: each of its values sets variables and the
/// root's `data-fs-opt-<key>`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeOption {
    pub key: String,
    pub label: String,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub value: String,
    pub label: String,
    #[serde(default)]
    pub vars: Vars,
}

fn grid() -> String {
    "grid".into()
}
fn title() -> String {
    "title".into()
}

impl Manifest {
    /// The layouts it allows, its own first.
    pub fn allowed_layouts(&self) -> Vec<&str> {
        let mut out = vec![self.layout.as_str()];
        out.extend(
            self.layouts
                .iter()
                .map(String::as_str)
                .filter(|l| *l != self.layout),
        );
        out
    }
}

/// Parse `text` as the manifest of the theme in folder `folder`, or say
/// why it is refused.
pub fn parse(text: &str, folder: &str) -> Result<Manifest, String> {
    let m: Manifest = serde_json::from_str(text).map_err(|e| format!("theme.json: {e}"))?;
    if m.api != API {
        return Err(format!("made for theme API {}, romforge has {API}", m.api));
    }
    if !slug(&m.id) {
        return Err(format!("id {:?}: lower-case letters, digits and - only", m.id));
    }
    if m.id != folder {
        return Err(format!("id {:?} isn't its folder's name ({folder:?})", m.id));
    }
    if m.name.trim().is_empty() {
        return Err("no name".into());
    }
    for l in m.allowed_layouts() {
        if !LAYOUTS.contains(&l) {
            return Err(format!("layout {l:?}: romforge has {}", LAYOUTS.join(", ")));
        }
    }
    if !FALLBACKS.contains(&m.fallback.as_str()) {
        return Err(format!(
            "fallback {:?}: one of {}",
            m.fallback,
            FALLBACKS.join(", ")
        ));
    }
    if m.schemes.is_empty() {
        return Err("no colour scheme".into());
    }
    for (name, vars) in &m.schemes {
        if !slug(name) {
            return Err(format!("scheme {name:?}: lower-case letters, digits and - only"));
        }
        check_vars(vars)?;
    }
    for o in &m.options {
        if !slug(&o.key) || o.choices.is_empty() {
            return Err(format!("option {:?}: a slug key and at least one choice", o.key));
        }
        for c in &o.choices {
            if !slug(&c.value) {
                return Err(format!("option {} value {:?}: a slug", o.key, c.value));
            }
            check_vars(&c.vars)?;
        }
    }
    Ok(m)
}

/// Lower-case ASCII letters, digits and inner dashes.
pub fn slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn check_vars(vars: &Vars) -> Result<(), String> {
    for (k, v) in vars {
        let name_ok = k.strip_prefix("--fs-").is_some_and(slug);
        if !name_ok {
            return Err(format!("variable {k:?}: themes set only --fs-… variables"));
        }
        if k == "--fs-scale" {
            return Err("--fs-scale is the user's text size, not the theme's".into());
        }
        let lower = v.to_ascii_lowercase();
        if v.contains([';', '{', '}', '\\', '<']) || lower.contains("url(") || lower.contains("//") {
            return Err(format!("variable {k}: {v:?} isn't a plain value"));
        }
    }
    Ok(())
}

/// Refuse CSS that could reach outside the theme or out of its scope:
/// `@import`, a URL with a scheme or `//`, escapes (which could spell
/// either), old script hooks, and braces that don't balance (a stray `}`
/// would close the `@scope` it is served in).
pub fn check_css(css: &str) -> Result<(), String> {
    let mut depth = 0i32;
    let mut text = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            if c == '\n' {
                return Err("a string runs past the end of its line".into());
            }
            if c == q {
                quote = None;
            }
            text.push(c);
            continue;
        }
        match c {
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut last = ' ';
                loop {
                    match chars.next() {
                        Some('/') if last == '*' => break,
                        Some(c) => last = c,
                        None => return Err("a comment isn't closed".into()),
                    }
                }
                text.push(' ');
            }
            '"' | '\'' => {
                quote = Some(c);
                text.push(c);
            }
            // A url(...) is one token to the browser, braces and all: read
            // it whole and allow only a plain relative path in it, so no
            // brace hides inside (one would close the @scope it is served
            // in).
            '(' if text.to_ascii_lowercase().ends_with("url") => {
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        Some(')') => break,
                        Some(c) => inner.push(c),
                        None => return Err("url( isn't closed".into()),
                    }
                }
                let path = inner.trim();
                let path = path
                    .strip_prefix('"')
                    .and_then(|p| p.strip_suffix('"'))
                    .or_else(|| path.strip_prefix('\'').and_then(|p| p.strip_suffix('\'')))
                    .unwrap_or(path);
                let plain = !path.is_empty()
                    && path
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
                    && !path.starts_with('/')
                    && !path.split('/').any(|p| p == "..");
                if !plain {
                    return Err(format!("theme.css: url({inner}) must be a file in the theme"));
                }
                text.push_str("(url)");
            }
            '{' => {
                depth += 1;
                text.push(c);
            }
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return Err("a } with no { before it".into());
                }
                text.push(c);
            }
            _ => text.push(c),
        }
    }
    if quote.is_some() {
        return Err("a string isn't closed".into());
    }
    if depth != 0 {
        return Err("a { isn't closed".into());
    }
    let lower = text.to_ascii_lowercase();
    for (bad, why) in [
        ("\\", "no escapes"),
        ("@import", "no @import"),
        ("//", "no URLs to other places (// or a scheme)"),
        ("expression(", "no expression()"),
        ("behavior", "no behavior"),
        ("-moz-binding", "no -moz-binding"),
        ("@namespace", "no @namespace"),
    ] {
        if lower.contains(bad) {
            return Err(format!("theme.css: {why}"));
        }
    }
    Ok(())
}
