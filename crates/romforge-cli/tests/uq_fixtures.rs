//! The answers in tests/uq/fixtures (what uQ Settings' romforge page is tested with) have the
//! shape `romforge-cli` really prints. `UPDATE_FIXTURES=1` is not needed: the fixtures are
//! hand-edited real output; this only compares which keys there are.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn run(db: &Path, args: &[&str]) -> Vec<u8> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let out = Command::new(env!("CARGO_BIN_EXE_romforge-cli"))
        .current_dir(&fixtures)
        .env("ROMFORGE_DB", db)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    out.stdout
}

fn json(db: &Path, args: &[&str]) -> Value {
    serde_json::from_slice(&run(db, args)).unwrap()
}

fn fixture(name: &str) -> Value {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/uq/fixtures")
        .join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// `a.b[].c` for every key, arrays flattened.
fn paths(v: &Value, at: &str, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(o) => {
            for (k, v) in o {
                let p = if at.is_empty() {
                    k.clone()
                } else {
                    format!("{at}.{k}")
                };
                out.insert(p.clone());
                paths(v, &p, out);
            }
        }
        Value::Array(a) => {
            for v in a {
                paths(v, &format!("{at}[]"), out);
            }
        }
        _ => {}
    }
}

fn shape(v: &Value) -> BTreeSet<String> {
    let mut s = BTreeSet::new();
    paths(v, "", &mut s);
    s
}

#[test]
fn the_page_fixtures_have_the_shape_of_the_real_answers() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("romforge.db");
    run(
        &db,
        &["dat", "import", "dats/nes.dat", "dats/gb.dat", "dats/snes.dat"],
    );
    // The real output only has an entry in bios[] with a BIOS list and emulators installed;
    // its keys are those of BiosGap, which this builds.
    let gap = serde_json::to_value(romforge_core::overview::BiosGap {
        system: "PlayStation".into(),
        name: "Sony PlayStation".into(),
        emulator: "DuckStation".into(),
    })
    .unwrap();
    let with_gap = |mut v: Value| {
        v["bios"] = Value::Array(vec![gap.clone()]);
        v
    };
    run(&db, &["scan", ".", "-q"]);

    let real = with_gap(json(&db, &["overview", "--json"]));
    for name in ["overview", "overview-empty", "overview-unidentified"] {
        let want = fixture(name);
        let want = if name == "overview" { want } else { with_gap(want) };
        let (a, b) = (shape(&want), shape(&real));
        // The empty fixtures have no games, platforms or recent rows to show keys of.
        let (a, b): (BTreeSet<_>, BTreeSet<_>) = if name == "overview" {
            (a, b)
        } else {
            let top = |s: &BTreeSet<String>| s.iter().filter(|p| !p.contains('[')).cloned().collect();
            (top(&a), top(&b))
        };
        assert_eq!(a, b, "{name}: keys differ from the real output");
    }
    assert_eq!(
        shape(&fixture("browse")),
        shape(&json(&db, &["browse", "all", "all", "title", "3", "0", "*"])),
    );
    let empty = json(&db, &["browse", "all", "all", "title", "3", "0", "zzzz"]);
    assert_eq!(shape(&fixture("browse-empty")), shape(&empty));
}
