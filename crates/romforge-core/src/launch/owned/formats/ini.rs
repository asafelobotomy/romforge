//! INI and TOML: `[Section]` then `Key = Value`; keys are named
//! `[Section]Key`.

/// `[Section]Key` split.
pub(super) fn split(key: &str) -> (&str, &str) {
    key.strip_prefix('[')
        .and_then(|k| k.split_once(']'))
        .unwrap_or(("", key))
}

pub(super) fn ini(text: &str, keys: &[(String, String)], remove: &[String]) -> String {
    let keys: Vec<(&str, &str, &str)> = keys
        .iter()
        .map(|(k, v)| {
            let (s, k) = split(k);
            (s, k, v.as_str())
        })
        .collect();
    let remove: Vec<(&str, &str)> = remove.iter().map(|k| split(k)).collect();
    let mut done = vec![false; keys.len()];
    let mut out = String::new();
    let mut section = String::new();
    // Keys of `section` not written yet, at its end.
    // (before the blank lines that end it).
    let flush = |out: &mut String, section: &str, done: &mut Vec<bool>| {
        let blanks = out.len() - out.trim_end_matches('\n').len();
        let keep = blanks.saturating_sub(1);
        out.truncate(out.len() - keep);
        for (i, (s, k, v)) in keys.iter().enumerate() {
            if !done[i] && *s == section {
                done[i] = true;
                out.push_str(&format!("{k} = {v}\n"));
            }
        }
        out.push_str(&"\n".repeat(keep));
    };
    for l in text.lines() {
        let t = l.trim();
        if let Some(name) = t.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            flush(&mut out, &section, &mut done);
            section = name.trim().to_owned();
        } else if let Some((k, _)) = t
            .split_once('=')
            .filter(|_| !t.starts_with(';') && !t.starts_with('#'))
        {
            let k = k.trim();
            if let Some(i) = keys.iter().position(|(s, key, _)| *s == section && *key == k) {
                if !done[i] {
                    done[i] = true;
                    out.push_str(&format!("{k} = {}\n", keys[i].2));
                }
                continue;
            }
            if remove.contains(&(section.as_str(), k)) {
                continue;
            }
        }
        out.push_str(l);
        out.push('\n');
    }
    flush(&mut out, &section, &mut done);
    // Sections the file doesn't have yet.
    for (i, (s, _, _)) in keys.iter().enumerate() {
        if !done[i] {
            if !out.is_empty() && !out.ends_with("\n\n") {
                out.push('\n');
            }
            if !s.is_empty() {
                out.push_str(&format!("[{s}]\n"));
            }
            flush(&mut out, s, &mut done);
        }
    }
    out
}
