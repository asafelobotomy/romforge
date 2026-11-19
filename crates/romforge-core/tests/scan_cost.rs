//! How long a rescan takes as the library grows. Not part of the normal
//! run (`cargo test --release -p romforge-core --test scan_cost -- --ignored
//! --nocapture`): it prints times, and asserts nothing about them.
#![cfg(unix)]

use romforge_core::{Library, Scope};
use std::time::Instant;

fn make(dir: &std::path::Path, n: usize) -> std::path::PathBuf {
    let roms = dir.join("roms");
    std::fs::create_dir(&roms).unwrap();
    let mut xml = String::from("<datafile><header><name>Nintendo - Game Boy</name></header>");
    for i in 0..n {
        let data = format!("rom number {i} {}", "x".repeat(1500));
        std::fs::write(roms.join(format!("g{i}.gb")), &data).unwrap();
        if i % 2 == 0 {
            let h = romforge_core::hash::hash_stream(data.as_bytes(), data.len() as u64)
                .unwrap()
                .raw;
            xml += &format!(
                r#"<game name="G{i}"><rom name="g{i}.gb" size="{}" sha1="{}"/></game>"#,
                h.size, h.sha1
            );
        }
    }
    xml += "</datafile>";
    std::fs::write(dir.join("x.dat"), xml).unwrap();
    roms
}

#[test]
#[ignore]
fn rescan_cost() {
    for n in [2_000usize, 10_000, 40_000] {
        let tmp = tempfile::tempdir().unwrap();
        let roms = make(tmp.path(), n);
        let dbp = tmp.path().join("lib.db");
        let mut lib = Library::open(&dbp).unwrap();
        lib.import_dat(tmp.path().join("x.dat"), None).unwrap();
        let t = Instant::now();
        lib.scan(&roms, Scope::All).unwrap();
        let first = t.elapsed();
        let t = Instant::now();
        lib.scan(&roms, Scope::All).unwrap();
        let unchanged = t.elapsed();
        std::fs::write(roms.join("new.gb"), "one new file").unwrap();
        let t = Instant::now();
        lib.scan(&roms, Scope::All).unwrap();
        let one_new = t.elapsed();
        println!("{n:>6} files: first {first:?}, unchanged rescan {unchanged:?}, one new file {one_new:?}");
    }
}
