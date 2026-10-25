//! Reading a config file back as (key, value) pairs, keys named as the
//! writers name them (`[Section]Key`, `Video/Renderer`, `Graphic/api`, a
//! flat key), values as written (TOML strings with their quotes, which the
//! caller keeps when writing back).

use super::Format;

pub fn read(format: Format, text: &str) -> Vec<(String, String)> {
    match format {
        Format::Ini | Format::Toml | Format::QtIni => ini(format, text),
        Format::NestedYaml => {
            let lines: Vec<&str> = text.lines().collect();
            super::nested_yaml::paths(&lines)
                .into_iter()
                .zip(&lines)
                .filter_map(|(p, l)| {
                    let (_, path) = p?;
                    let (_, _, v) = super::nested_yaml::mapping(l)?;
                    // A map's own line (`Video:`) has no value of its own.
                    (!v.is_empty()).then(|| (path.join("/"), v.to_owned()))
                })
                .collect()
        }
        Format::Xml => xml(text),
        _ => text.lines().filter_map(|l| flat(format, l)).collect(),
    }
}

fn flat(format: Format, l: &str) -> Option<(String, String)> {
    let t = l.trim();
    if t.is_empty() || t.starts_with('#') || t.starts_with(';') {
        return None;
    }
    match format {
        Format::RetroArch => {
            let (k, v) = t.split_once('=')?;
            Some((k.trim().to_owned(), v.trim().trim_matches('"').to_owned()))
        }
        Format::Yaml => {
            if l.starts_with([' ', '\t']) {
                return None;
            }
            let (k, v) = t.split_once(':')?;
            Some((k.trim().trim_matches('"').to_owned(), v.trim().to_owned()))
        }
        _ => {
            let (k, v) = t.split_once(char::is_whitespace).unwrap_or((t, ""));
            Some((k.to_owned(), v.trim().to_owned()))
        }
    }
}

fn ini(format: Format, text: &str) -> Vec<(String, String)> {
    let mut section = String::new();
    let mut out = Vec::new();
    for l in text.lines() {
        let t = l.trim();
        if let Some(name) = t.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            section = name.trim().to_owned();
            continue;
        }
        if t.starts_with(';') || t.starts_with('#') {
            continue;
        }
        let Some((k, v)) = t.split_once('=') else { continue };
        let k = k.trim();
        if format == Format::QtIni && k.ends_with("\\default") {
            continue;
        }
        let key = if section.is_empty() {
            k.to_owned()
        } else {
            format!("[{section}]{k}")
        };
        out.push((key, v.trim().to_owned()));
    }
    out
}

/// Leaf elements under `<content>` that hold text, by path; a path that
/// repeats (a list's entries) is left out, as it names no one element.
fn xml(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut repeated: Vec<String> = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('<') {
        let after = &rest[i + 1..];
        let Some(end) = after.find('>') else { break };
        let tag = &after[..end];
        let text_after = &after[end + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
        } else if let Some(name) = tag.strip_prefix('/') {
            if stack.last().is_some_and(|n| n == name.trim()) {
                stack.pop();
            }
        } else if tag.ends_with('/') {
        } else {
            let name = tag.split_whitespace().next().unwrap_or(tag).to_owned();
            stack.push(name.clone());
            let close = format!("</{name}>");
            if let Some(c) = text_after.find(&close)
                && !text_after[..c].contains('<')
                && stack.first().is_some_and(|r| r == "content")
                && stack.len() > 1
            {
                let path = stack[1..].join("/");
                let value = text_after[..c]
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&amp;", "&");
                if out.iter().any(|(p, _)| *p == path) {
                    repeated.push(path);
                } else {
                    out.push((path, value));
                }
            }
        }
        rest = text_after;
    }
    out.retain(|(p, _)| !repeated.contains(p));
    out
}
