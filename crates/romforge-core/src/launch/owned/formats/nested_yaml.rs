//! RPCS3's `config.yml`: nested maps (`Video:` then `  Renderer: Vulkan`), a
//! key its path (`Video/Renderer`). A key there gets its value where it is,
//! its indentation kept; a missing one is added at the end of its parent's
//! block (the parents too, where missing), indented as its siblings are.

/// A mapping line: (indentation, key, the rest after `:`).
pub(super) fn mapping(line: &str) -> Option<(usize, &str, &str)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let t = &line[indent..];
    if t.is_empty() || t.starts_with('#') || t.starts_with('-') {
        return None;
    }
    let (k, rest) = t.split_once(':')?;
    // A `:` inside a quoted key isn't one of ours; RPCS3's keys aren't quoted.
    if k.starts_with('"') || k.starts_with('\'') {
        return None;
    }
    Some((indent, k.trim(), rest.trim()))
}

/// Each line's map path (mapping lines only), in order.
pub(super) fn paths(lines: &[&str]) -> Vec<Option<(usize, Vec<String>)>> {
    let mut stack: Vec<(usize, String)> = Vec::new();
    lines
        .iter()
        .map(|l| {
            let (indent, k, _) = mapping(l)?;
            while stack.last().is_some_and(|(i, _)| *i >= indent) {
                stack.pop();
            }
            stack.push((indent, k.to_owned()));
            Some((indent, stack.iter().map(|(_, k)| k.clone()).collect()))
        })
        .collect()
}

pub(super) fn write(text: &str, keys: &[(String, String)]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    for (key, value) in keys {
        let target: Vec<&str> = key.split('/').filter(|p| !p.is_empty()).collect();
        if target.is_empty() {
            continue;
        }
        let view: Vec<&str> = lines.iter().map(String::as_str).collect();
        let found = paths(&view);
        // Where it is.
        if let Some(i) = found
            .iter()
            .position(|p| p.as_ref().is_some_and(|(_, p)| p == &target))
        {
            let indent = found[i].as_ref().map_or(0, |(n, _)| *n);
            lines[i] = format!("{}{}: {value}", " ".repeat(indent), target[target.len() - 1]);
            continue;
        }
        // The deepest parent that is there, the end of its block, and how
        // its children are indented.
        let mut best: (usize, Option<usize>, usize) = (0, None, 0); // (depth, last line, child indent)
        for depth in (1..target.len()).rev() {
            let prefix = &target[..depth];
            let Some(start) = found
                .iter()
                .position(|p| p.as_ref().is_some_and(|(_, p)| p.as_slice() == prefix))
            else {
                continue;
            };
            let own = found[start].as_ref().map_or(0, |(n, _)| *n);
            let mut last = start;
            let mut child = None;
            for (j, l) in view.iter().enumerate().skip(start + 1) {
                let indent = l.len() - l.trim_start_matches(' ').len();
                if l.trim().is_empty() {
                    continue;
                }
                if indent <= own {
                    break;
                }
                last = j;
                if child.is_none() && found[j].is_some() {
                    child = Some(indent);
                }
            }
            best = (depth, Some(last), child.unwrap_or(own + 2));
            break;
        }
        let (depth, last, mut indent) = best;
        let mut piece = Vec::new();
        for (j, seg) in target[depth..].iter().enumerate() {
            if depth + j + 1 == target.len() {
                piece.push(format!("{}{seg}: {value}", " ".repeat(indent)));
            } else {
                piece.push(format!("{}{seg}:", " ".repeat(indent)));
            }
            indent += 2;
        }
        let at = last.map_or(lines.len(), |l| l + 1);
        lines.splice(at..at, piece);
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// `text` without the values at `keys` (a map, with lines under it, stays).
pub(super) fn remove(text: &str, keys: &[String]) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let found = paths(&lines);
    let gone = |i: usize| {
        found[i].as_ref().is_some_and(|(_, p)| {
            !mapping(lines[i]).is_some_and(|(_, _, rest)| rest.is_empty())
                && keys.iter().any(|k| {
                    k.split('/')
                        .filter(|s| !s.is_empty())
                        .eq(p.iter().map(String::as_str))
                })
        })
    };
    let mut out: String = (0..lines.len())
        .filter(|&i| !gone(i))
        .map(|i| format!("{}\n", lines[i]))
        .collect();
    if text.is_empty() {
        out.clear();
    }
    out
}
