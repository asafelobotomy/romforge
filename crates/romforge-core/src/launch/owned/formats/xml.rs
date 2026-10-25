//! Cemu's `settings.xml`: elements under `<content>`, a key their path from
//! it (`mlc_path`, `Graphic/api`, `Graphic/Overlay/FPS`). An element there
//! gets its new text; a missing one is added inside its parent (the parent
//! too, where missing); no file gives a new one. Elements of the same name
//! nested in each other aren't expected (Cemu has none).

/// Escape text for XML.
fn esc(v: &str) -> String {
    v.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Where element `name` is within `within` of `doc`: (start of its opening
/// tag, end of its opening tag, start of its closing tag, end of it). An
/// empty element (`<x/>`) has the same inner start and end.
pub(super) fn find(doc: &str, within: (usize, usize), name: &str) -> Option<(usize, usize, usize, usize)> {
    let (from, to) = within;
    let region = doc.get(from..to)?;
    let mut at = 0;
    while let Some(i) = region[at..].find(&format!("<{name}")) {
        let start = at + i;
        let after = &region[start + 1 + name.len()..];
        match after.chars().next() {
            Some('>') | Some(' ') | Some('/') | Some('\t') | Some('\n') => {}
            _ => {
                at = start + 1;
                continue;
            }
        }
        let open_end = start + region[start..].find('>')? + 1;
        if region[..open_end].ends_with("/>") {
            return Some((from + start, from + open_end, from + open_end, from + open_end));
        }
        let close = format!("</{name}>");
        let c = open_end + region[open_end..].find(&close)?;
        return Some((from + start, from + open_end, from + c, from + c + close.len()));
    }
    None
}

/// The region inside `<content>`.
fn content(doc: &str) -> Option<(usize, usize)> {
    let (_, inner, close, _) = find(doc, (0, doc.len()), "content")?;
    Some((inner, close))
}

/// `text` without the elements at `keys`, and their lines when nothing
/// else is on them.
pub(super) fn remove(text: &str, keys: &[String]) -> String {
    let mut out = text.to_owned();
    for k in keys {
        let Some(mut region) = content(&out) else { break };
        let parts: Vec<&str> = k.split('/').filter(|p| !p.is_empty()).collect();
        for (i, part) in parts.iter().enumerate() {
            let Some((s, inner, close, end)) = find(&out, region, part) else {
                break;
            };
            if i + 1 < parts.len() {
                region = (inner, close);
                continue;
            }
            let line_start = out[..s].rfind('\n').map_or(0, |n| n + 1);
            let line_end = out[end..].find('\n').map_or(out.len(), |n| end + n + 1);
            let alone = out[line_start..s].trim().is_empty() && out[end..line_end].trim().is_empty();
            if alone {
                out.replace_range(line_start..line_end, "");
            } else {
                out.replace_range(s..end, "");
            }
        }
    }
    out
}

pub(super) fn xml(text: &str, keys: &[(String, String)]) -> String {
    let mut out = if text.contains("</content>") {
        text.to_owned()
    } else {
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<content>\n</content>\n".to_owned()
    };
    for (k, v) in keys {
        let parts: Vec<&str> = k.split('/').filter(|p| !p.is_empty()).collect();
        let Some(mut region) = content(&out) else { break };
        for (i, part) in parts.iter().enumerate() {
            let depth = i + 1;
            let leaf = i + 1 == parts.len();
            match find(&out, region, part) {
                Some((s, _, _, end)) if leaf => {
                    let full = format!("<{part}>{}</{part}>", esc(v));
                    out.replace_range(s..end, &full);
                }
                Some((_, inner, close, end)) => {
                    if inner == close && inner == end {
                        // `<Parent/>`: open it up.
                        let s = out[..end].rfind('<').unwrap_or(end);
                        out.replace_range(s..end, &format!("<{part}>\n{}</{part}>", "    ".repeat(depth)));
                        region = match find(&out, (s, out.len()), part) {
                            Some((_, i2, c2, _)) => (i2, c2),
                            None => break,
                        };
                    } else {
                        region = (inner, close);
                    }
                }
                None => {
                    // Add the rest of the path inside `region`, before its end.
                    let mut piece = String::new();
                    for (j, p) in parts[i..].iter().enumerate() {
                        let pad = "    ".repeat(depth + j);
                        if i + j + 1 == parts.len() {
                            piece.push_str(&format!("{pad}<{p}>{}</{p}>\n", esc(v)));
                        } else {
                            piece.push_str(&format!("{pad}<{p}>\n"));
                        }
                    }
                    for (j, p) in parts[i..parts.len() - 1].iter().enumerate().rev() {
                        piece.push_str(&format!("{}</{p}>\n", "    ".repeat(depth + j)));
                    }
                    // Before the closing tag's line (its indentation kept).
                    let at = out[..region.1]
                        .rfind('\n')
                        .map_or(region.1, |n| n + 1)
                        .max(region.0);
                    let at = if out[at..region.1].trim().is_empty() {
                        at
                    } else {
                        region.1
                    };
                    out.insert_str(at, &piece);
                    break;
                }
            }
        }
    }
    out
}
