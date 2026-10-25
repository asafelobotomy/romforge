//! Writing keys into emulators' config files, each in its own format, with
//! everything else in the file (other keys, comments, order) kept.

use crate::Error;

/// A config file's format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// RetroArch: `key = "value"`.
    RetroArch,
    /// INI: `[Section]` then `Key = Value`; keys are written `[Section]Key`.
    Ini,
    /// Mednafen: `key value`.
    Spaced,
    /// TOML: `[table]` then `key = value`, keys written `[table]key`, values
    /// as TOML (strings quoted by [`toml_string`]).
    Toml,
    /// RPCS3's flat YAML maps: `key: value` at the top level (values quoted
    /// by [`toml_string`], which YAML reads the same).
    Yaml,
    /// Cemu's `settings.xml`: elements inside `<content>`, keys their path
    /// from it (`mlc_path`, `Graphic/api`).
    Xml,
    /// RPCS3's `config.yml`: nested maps, keys their path (`Video/Renderer`,
    /// `Video/Performance Overlay/Enabled`).
    NestedYaml,
    /// Qt's INI (Azahar, Lime3DS, Citra): INI, and every key written also
    /// gets `key\default = false`, or Qt reads its default instead.
    QtIni,
}

mod flat;
mod ini;
mod nested_yaml;
mod read;
mod xml;
use flat::flat;
use ini::ini;
pub use read::read;
use xml::xml;

/// `s` as a TOML basic string.
pub fn toml_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `text` with `keys` set and the keys in `remove` gone: a key's line is
/// replaced where it is, a key with no line is added (at its section's end
/// for INI).
pub fn merge(
    format: Format,
    text: &str,
    keys: &[(String, String)],
    remove: &[String],
) -> Result<String, Error> {
    for (k, v) in keys {
        if v.contains('\n') || (format == Format::RetroArch && v.contains('"')) {
            return Err(Error::Launch(format!(
                "{k}: can't write {v:?} into a config file"
            )));
        }
    }
    Ok(match format {
        Format::Ini | Format::Toml => ini(text, keys, remove),
        Format::QtIni => {
            let remove: Vec<String> = remove
                .iter()
                .flat_map(|k| [k.clone(), format!("{k}\\default")])
                .collect();
            ini(text, &with_defaults(keys), &remove)
        }
        Format::Xml => xml(&xml::remove(text, remove), keys),
        Format::NestedYaml => nested_yaml::write(&nested_yaml::remove(text, remove), keys),
        _ => flat(format, text, keys, remove),
    })
}

/// Qt's `key\default = false` beside each key not given one already.
fn with_defaults(keys: &[(String, String)]) -> Vec<(String, String)> {
    let mut out = keys.to_vec();
    for (k, _) in keys {
        if !k.ends_with("\\default") {
            let d = format!("{k}\\default");
            if !keys.iter().any(|(x, _)| *x == d) {
                out.push((d, "false".to_owned()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
