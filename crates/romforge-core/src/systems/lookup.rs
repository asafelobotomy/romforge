//! Finding a system in the table: by key, extension, id, header kind or
//! DAT name.

use super::{SYSTEMS, System};

/// Find a system by its key (case-insensitive), e.g. `NES` or `game boy`.
pub fn by_key(key: &str) -> Option<&'static System> {
    SYSTEMS.iter().find(|s| s.key.eq_ignore_ascii_case(key.trim()))
}

/// Whether any system uses this extension (case-insensitive, no dot).
pub fn is_rom_extension(ext: &str) -> bool {
    SYSTEMS
        .iter()
        .any(|s| s.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
}

/// The one system using this extension (case-insensitive, no dot); `None`
/// when none or several do (`.bin`). A hint about an unknown file, never
/// a match (house rule 8).
pub fn by_extension(ext: &str) -> Option<&'static System> {
    let mut it = SYSTEMS
        .iter()
        .filter(|s| s.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)));
    let one = it.next()?;
    it.next().is_none().then_some(one)
}

pub fn by_id(id: i64) -> Option<&'static System> {
    SYSTEMS.iter().find(|s| s.id == id)
}

/// Systems whose games carry a game ID of `kind` ("snes", "ps1", ...).
pub fn by_ident(kind: &str) -> impl Iterator<Item = &'static System> {
    SYSTEMS.iter().filter(move |s| s.ident.contains(&kind))
}

/// Guess a system from a DAT header name such as
/// `Nintendo - Nintendo Entertainment System (Headered) (Parent-Clone)`.
/// Trailing `(...)` groups are ignored and the rest must match exactly, so
/// `Nintendo - Game Boy Color` never matches `Nintendo - Game Boy`.
pub fn from_dat_name(dat_name: &str) -> Option<&'static System> {
    let base = strip_paren_suffixes(dat_name);
    SYSTEMS
        .iter()
        .find(|s| s.dat_names.iter().any(|n| n.eq_ignore_ascii_case(base)))
}

fn strip_paren_suffixes(mut s: &str) -> &str {
    loop {
        s = s.trim_end();
        if s.ends_with(')')
            && let Some(open) = s.rfind(" (")
        {
            s = &s[..open];
            continue;
        }
        return s;
    }
}
