//! The user's pick of theme, colour scheme, layout, text size and the
//! theme's options, kept in one `settings` row (each theme keeps its own
//! options), and what the page applies for it.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use super::{DEFAULT, Manifest, Vars, find, stamp};
use crate::Error;

/// The `settings` row it is kept in.
const KEY: &str = "fs_theme";

/// Text sizes, in percent of the theme's own.
pub const TEXT_SIZES: &[u32] = &[90, 100, 115, 130];

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Selection {
    pub theme: String,
    /// Empty: the system's light or dark, where the theme has both, else
    /// its first scheme.
    pub scheme: String,
    /// Empty: the theme's own.
    pub layout: String,
    pub text_size: u32,
    /// Theme id → option key → value.
    pub options: BTreeMap<String, BTreeMap<String, String>>,
}

impl Default for Selection {
    fn default() -> Self {
        Selection {
            theme: DEFAULT.into(),
            scheme: String::new(),
            layout: String::new(),
            text_size: 100,
            options: BTreeMap::new(),
        }
    }
}

/// What the fullscreen mode applies.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Applied {
    pub theme: String,
    /// For `theme://localhost/<theme>/theme.css?v=<stamp>`.
    pub stamp: u64,
    pub layout: String,
    pub fallback: String,
    /// The scheme set, or none to follow the system's (then `schemes`
    /// holds "light" and "dark").
    pub scheme: Option<String>,
    pub schemes: BTreeMap<String, Vars>,
    /// The options' variables, set after the scheme's.
    pub vars: Vars,
    /// `data-fs-opt-<key>` = value, for each option.
    pub attrs: BTreeMap<String, String>,
    /// 1.0 is the theme's own size.
    pub scale: f32,
}

pub fn load(conn: &Connection) -> Result<Selection, Error> {
    let text: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [KEY], |r| r.get(0))
        .optional()?;
    Ok(text
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default())
}

pub fn save(conn: &Connection, s: &Selection) -> Result<(), Error> {
    let text = serde_json::to_string(s).map_err(|e| Error::Settings(e.to_string()))?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![KEY, text],
    )?;
    Ok(())
}

/// `s` made to fit what there is: a theme that is gone or refused is
/// [`DEFAULT`]; an unknown scheme, layout, option value or size is the
/// theme's own. Returns the theme's manifest too.
pub fn fit(dir: Option<&Path>, s: &Selection) -> (Selection, Manifest) {
    let (theme, m) = match find(dir, &s.theme) {
        Some(m) => (s.theme.clone(), m),
        None => (
            DEFAULT.to_owned(),
            find(None, DEFAULT).expect("the default theme is built in and valid"),
        ),
    };
    let scheme = Some(s.scheme.clone()).filter(|k| m.schemes.contains_key(k));
    let layout = Some(s.layout.clone()).filter(|l| m.allowed_layouts().contains(&l.as_str()));
    let text_size = if TEXT_SIZES.contains(&s.text_size) {
        s.text_size
    } else {
        100
    };
    let mut options = s.options.clone();
    let mine = options.entry(theme.clone()).or_default();
    mine.retain(|k, v| {
        m.options
            .iter()
            .any(|o| o.key == *k && o.choices.iter().any(|c| c.value == *v))
    });
    let out = Selection {
        theme,
        scheme: scheme.unwrap_or_default(),
        layout: layout.unwrap_or_default(),
        text_size,
        options,
    };
    (out, m)
}

/// What the page applies for `s`.
pub fn applied(dir: Option<&Path>, s: &Selection) -> Applied {
    let (s, m) = fit(dir, s);
    let follow = s.scheme.is_empty() && m.schemes.contains_key("light") && m.schemes.contains_key("dark");
    let scheme = if follow {
        None
    } else if s.scheme.is_empty() {
        m.schemes.keys().next().cloned()
    } else {
        Some(s.scheme.clone())
    };
    let schemes = m
        .schemes
        .iter()
        .filter(|(k, _)| match &scheme {
            Some(name) => *k == name,
            None => *k == "light" || *k == "dark",
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let chosen = s.options.get(&s.theme);
    let (mut vars, mut attrs) = (Vars::new(), BTreeMap::new());
    for o in &m.options {
        let value = chosen.and_then(|c| c.get(&o.key));
        let c = o
            .choices
            .iter()
            .find(|c| Some(&c.value) == value)
            .unwrap_or(&o.choices[0]);
        vars.extend(c.vars.clone());
        attrs.insert(o.key.clone(), c.value.clone());
    }
    Applied {
        stamp: stamp(dir, &s.theme),
        layout: if s.layout.is_empty() {
            m.layout.clone()
        } else {
            s.layout.clone()
        },
        fallback: m.fallback.clone(),
        theme: s.theme,
        scheme,
        schemes,
        vars,
        attrs,
        scale: s.text_size as f32 / 100.0,
    }
}
