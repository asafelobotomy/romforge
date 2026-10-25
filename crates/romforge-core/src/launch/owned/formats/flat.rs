//! One `key value` per line: RetroArch's `key = "value"`, Mednafen's and
//! MAME's `key value`, RPCS3's flat top-level YAML (`vfs.yml`).

use super::Format;

pub(super) fn flat(format: Format, text: &str, keys: &[(String, String)], remove: &[String]) -> String {
    let line = |k: &str, v: &str| match format {
        Format::RetroArch => format!("{k} = \"{v}\""),
        Format::Yaml => format!("{k}: {v}"),
        _ => format!("{k} {v}"),
    };
    let key_of = |l: &str| -> Option<String> {
        match format {
            Format::RetroArch => l.split_once('=').map(|(k, _)| k.trim().to_owned()),
            // Top-level keys only (nested maps are indented).
            Format::Yaml if l.starts_with([' ', '\t', '#']) => None,
            Format::Yaml => l
                .split_once(':')
                .map(|(k, _)| k.trim().trim_matches('"').to_owned()),
            _ => l
                .split_whitespace()
                .next()
                .filter(|k| !k.starts_with(';'))
                .map(str::to_owned),
        }
    };
    let mut done = vec![false; keys.len()];
    let mut out = String::new();
    for l in text.lines() {
        match key_of(l).and_then(|k| keys.iter().position(|(key, _)| *key == k)) {
            Some(i) if !done[i] => {
                done[i] = true;
                out.push_str(&line(&keys[i].0, &keys[i].1));
            }
            Some(_) => continue,
            None if key_of(l).is_some_and(|k| remove.contains(&k)) => continue,
            None => out.push_str(l),
        }
        out.push('\n');
    }
    for (i, (k, v)) in keys.iter().enumerate() {
        if !done[i] {
            out.push_str(&line(k, v));
            out.push('\n');
        }
    }
    out
}
