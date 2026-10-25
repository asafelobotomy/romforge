//! Emulator names written as strings in the code that branches on them
//! (`match name { "duckstation" => … }`, `matches!(name, …)`,
//! `settings_name() == "…"`, Emulator Settings' rows) must each be an
//! emulator of the launch table (house rule 4): a misspelt one compiles
//! and silently never matches.

use std::path::{Path, PathBuf};

use romforge_core::launch::{KNOWN_EMULATORS, RETROARCH};

/// The folders whose code branches on emulator names.
const ROOTS: &[&str] = &["launch", "saves", "emusettings", "library"];
/// What such a `match` or `matches!` is on.
const SUBJECTS: &[&str] = &["name", "emulator", "e.name", "e.settings_name()"];
/// Emulator Settings' row builders, the emulator first.
const ROWS: &[&str] = &["k(\"", "ini(\"", "toml(\"", "spaced(\""];

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        let name = e.file_name();
        if p.is_dir() {
            if name != "tests" {
                walk(&p, out);
            }
        } else if p.extension().is_some_and(|x| x == "rs") && name != "tests.rs" {
            out.push(p);
        }
    }
}

/// The quoted strings in `s`.
fn quoted(s: &str) -> Vec<&str> {
    s.split('"').skip(1).step_by(2).collect()
}

/// The names in `file`'s code (its tests left out) that are written where
/// an emulator's name goes, with their line numbers.
fn names_in(text: &str) -> Vec<(usize, String)> {
    let code = text.split("#[cfg(test)]").next().unwrap_or(text);
    let mut out = Vec::new();
    // Inside a `match <subject> {`: the depth of its braces.
    let mut in_match: Option<i32> = None;
    for (i, line) in code.lines().enumerate() {
        let t = line.trim();
        if t.starts_with("//") {
            continue;
        }
        let n = i + 1;
        if let Some(depth) = in_match.as_mut() {
            let arm = t.trim_start_matches('|').trim_start();
            if *depth == 1 && arm.starts_with('"') {
                let head = arm.split("=>").next().unwrap_or(arm);
                let head = head.split(" if ").next().unwrap_or(head);
                if arm.contains("=>") || head.trim_end().ends_with('|') {
                    out.extend(quoted(head).into_iter().map(|q| (n, q.to_owned())));
                }
            }
            *depth += t.matches('{').count() as i32 - t.matches('}').count() as i32;
            if *depth <= 0 {
                in_match = None;
            }
            continue;
        }
        if SUBJECTS.iter().any(|s| t.contains(&format!("match {s} {{"))) {
            in_match = Some(t.matches('{').count() as i32 - t.matches('}').count() as i32);
            continue;
        }
        for s in SUBJECTS {
            if let Some(rest) = t.split(&format!("matches!({s},")).nth(1) {
                let args = rest.split(')').next().unwrap_or(rest);
                out.extend(quoted(args).into_iter().map(|q| (n, q.to_owned())));
            }
        }
        for cmp in ["settings_name() == \"", "e.name == \""] {
            for part in t.split(cmp).skip(1) {
                out.push((n, part.split('"').next().unwrap_or("").to_owned()));
            }
        }
        for row in ROWS {
            for part in t.split(row).skip(1) {
                let before = t.split(row).next().unwrap_or("");
                // Only a call, not a longer name ending the same way.
                if before.ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                out.push((n, part.split('"').next().unwrap_or("").to_owned()));
            }
        }
    }
    out
}

#[test]
fn every_emulator_name_in_the_code_is_in_the_launch_table() {
    let known: Vec<&str> = std::iter::once(&RETROARCH)
        .chain(KNOWN_EMULATORS)
        .map(|e| e.name)
        .collect();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    for root in ROOTS {
        walk(&src.join(root), &mut files);
    }
    let mut seen = 0;
    let mut unknown = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        for (line, name) in names_in(&text) {
            seen += 1;
            if !known.contains(&name.as_str()) {
                let rel = f.strip_prefix(&src).unwrap_or(f);
                unknown.push(format!("src/{}:{line}: \"{name}\"", rel.display()));
            }
        }
    }
    assert!(
        seen > 100,
        "the scan found only {seen} names: it has stopped finding them"
    );
    assert!(
        unknown.is_empty(),
        "not an emulator of the launch table:\n{}",
        unknown.join("\n")
    );
}

#[test]
fn a_misspelt_name_is_caught() {
    let code = "fn f(name: &str) {\n    match name {\n        \"duckstaton\" => {}\n        \"a\" | \"b\" => {}\n        _ => {}\n    }\n    matches!(name, \"pcsx\");\n}\n";
    let names: Vec<String> = names_in(code).into_iter().map(|(_, n)| n).collect();
    assert_eq!(names, ["duckstaton", "a", "b", "pcsx"]);
}
