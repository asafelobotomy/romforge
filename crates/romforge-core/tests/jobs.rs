//! Long jobs: progress with a total, stopping at a safe point, and the
//! threads setting.

use romforge_core::job::Cancel;
use romforge_core::{Error, Library, ScanOptions, Scope};

mod common;
use common::fixture_lib::*;
use common::fixtures;

/// A scan says how many files it will read before reading any, and ends
/// with all of them done.
#[test]
fn scan_reports_its_total_first() {
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let mut seen = Vec::new();
    lib.scan_with(
        fixtures().join("roms"),
        Scope::All,
        &ScanOptions::default(),
        |p| seen.push(p),
    )
    .unwrap();
    let first = seen.first().unwrap();
    assert!(first.units_total > 0 && first.units_done == 0, "{first:?}");
    assert!(first.disk_total > 0);
    let last = seen.last().unwrap();
    assert_eq!(
        (last.units_done, last.disk_done),
        (last.units_total, last.disk_total)
    );
    assert!(seen.iter().all(|p| p.units_total == first.units_total));
    let step = last.step();
    assert_eq!(step.done, step.total, "{step:?}");
}

/// A scan asked to stop leaves the library as it was: its transaction
/// rolls back.
#[test]
fn a_stopped_scan_changes_nothing() {
    let mut lib = Library::open_in_memory().unwrap();
    import_all(&mut lib);
    let before = by_path(&lib);
    let cancel = Cancel::new();
    let opts = ScanOptions {
        jobs: 1,
        cancel: cancel.clone(),
        ..ScanOptions::default()
    };
    let mut calls = 0;
    let r = lib.scan_with(fixtures().join("roms"), Scope::All, &opts, |p| {
        calls += 1;
        // Stop once the first file is done.
        if p.units_done == 1 {
            cancel.cancel();
        }
    });
    assert!(matches!(r, Err(Error::Cancelled)), "{r:?}");
    assert!(calls > 0);
    assert_eq!(by_path(&lib), before);
    assert!(lib.can_refresh().is_ok());

    // Refresh stops too, instead of listing the stop as one folder's error.
    let done = Cancel::new();
    done.cancel();
    let opts = ScanOptions {
        cancel: done,
        ..ScanOptions::default()
    };
    lib.scan(fixtures().join("roms/gb"), Scope::All).unwrap();
    assert!(matches!(lib.refresh(&opts, |_, _| {}), Err(Error::Cancelled)));
}

/// The threads setting is stored, and a scan with it agrees with one
/// thread.
#[test]
fn threads_setting_round_trips() {
    let mut lib = Library::open_in_memory().unwrap();
    let s = romforge_core::settings::Settings {
        threads: 3,
        ..Default::default()
    };
    assert_eq!(lib.save_settings(&s).unwrap().threads, 3);
    assert_eq!(lib.settings().unwrap().threads, 3);
    let too_many = romforge_core::settings::Settings {
        threads: 1000,
        ..Default::default()
    };
    assert!(lib.save_settings(&too_many).is_err());
    import_all(&mut lib);
    // Rows are compared without their ids, which follow hashing order.
    let rows = |lib: &Library| {
        by_path(lib)
            .into_iter()
            .map(|(p, f)| (p, f.status, f.games))
            .collect::<Vec<_>>()
    };
    lib.scan(fixtures().join("roms"), Scope::All).unwrap();
    let three = rows(&lib);
    let mut one = Library::open_in_memory().unwrap();
    import_all(&mut one);
    let serial = ScanOptions {
        jobs: 1,
        ..ScanOptions::default()
    };
    one.scan_with(fixtures().join("roms"), Scope::All, &serial, |_| {})
        .unwrap();
    assert_eq!(rows(&one), three);
}

fn cleanup_setup(threads: usize) -> common::cleanup::Setup {
    let mut s = common::cleanup::setup(
        false,
        &[
            ("gb/Test GB.gb", "loose/tgb.gb"),
            ("archives/Test GBA.zip", "Test GBA.zip"),
            ("archives/Mixed.7z", "Mixed.7z"),
            ("nes/Test NES (iNES).nes", "Test NES (iNES).nes"),
        ],
        &["gb.dat", "gba.dat", "md.dat", "nes.dat"],
    );
    let settings = romforge_core::settings::Settings {
        threads,
        ..s.lib.settings().unwrap()
    };
    s.lib.save_settings(&settings).unwrap();
    s
}

/// Every file under `dir` (relative) with its size, and a zip's members.
fn tree(dir: &std::path::Path) -> Vec<(String, u64, Vec<String>)> {
    let mut out: Vec<_> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            let rel = e.path().strip_prefix(dir).unwrap().to_string_lossy().into_owned();
            let zip = if rel.ends_with(".zip") {
                common::cleanup::zip_names(e.path())
            } else {
                Vec::new()
            };
            (rel, e.metadata().unwrap().len(), zip)
        })
        .collect();
    out.sort();
    out
}

/// Games written on four threads come out as they do on one.
#[test]
fn cleanup_on_four_threads_matches_one() {
    let run = |threads| {
        let mut s = cleanup_setup(threads);
        let plan = s.lib.cleanup_plan(None).unwrap();
        assert!(plan.items.len() >= 3, "{:?}", plan.items);
        let mut order = Vec::new();
        let r = s.lib.cleanup_apply(&plan, |n, _| order.push(n)).unwrap();
        assert!(r.failed.is_empty() && !r.stopped, "{r:?}");
        assert_eq!(order, (0..plan.items.len()).collect::<Vec<_>>(), "in plan order");
        (r.done, r.trashed, tree(&s.library))
    };
    assert_eq!(run(4), run(1));
}

/// A cleanup stopped after its first game keeps that game (journaled, its
/// original trashed), leaves the rest alone, and undo takes it back.
#[test]
fn a_stopped_cleanup_keeps_what_it_finished() {
    let mut s = cleanup_setup(1);
    let plan = s.lib.cleanup_plan(None).unwrap();
    let before = tree(&s.source);
    let cancel = Cancel::new();
    let r = s
        .lib
        .cleanup_apply_with(&plan, &cancel, |n, _| {
            if n == 0 {
                cancel.cancel();
            }
        })
        .unwrap();
    assert!(r.stopped, "{r:?}");
    assert_eq!(r.done, 1, "{r:?}");
    assert!(r.failed.is_empty(), "{r:?}");
    assert!(plan.items[0].target.exists());
    assert!(plan.items[1..].iter().all(|i| !i.target.exists()));
    let undone = s.lib.cleanup_undo(None).unwrap();
    assert!(undone.failed.is_empty(), "{:?}", undone.failed);
    assert!(!plan.items[0].target.exists());
    assert_eq!(tree(&s.source), before);
}

/// Enrich against a stand-in libretro that has box art for every game.
fn enrich_run(
    threads: usize,
    cancel: Cancel,
) -> (
    romforge_core::enrich::EnrichReport,
    Vec<romforge_core::job::Step>,
    Vec<(String, u64)>,
) {
    use romforge_core::enrich::{ArtKind, EnrichEvent, EnrichOptions};
    let (base, _log) = common::serve::serve(|path| {
        path.contains("/Named_Boxarts/")
            .then(|| [b"\x89PNG\r\n\x1a\n".as_slice(), path.as_bytes()].concat())
    });
    let mut lib = Library::open_in_memory().unwrap();
    lib.import_dat(fixtures().join("dats/gb.dat"), None).unwrap();
    lib.scan(fixtures().join("roms/gb"), Scope::All).unwrap();
    let cache = tempfile::tempdir().unwrap();
    let mut opts = EnrichOptions::new(cache.path());
    opts.metadata_base = format!("{base}/meta");
    opts.thumbnails_base = format!("{base}/thumbs");
    opts.art = vec![ArtKind::Boxart, ArtKind::Snap];
    opts.wikidata = None;
    opts.threads = threads;
    opts.cancel = cancel.clone();
    let mut steps = Vec::new();
    let report = lib
        .enrich(&opts, |e| {
            if let EnrichEvent::Progress(s) = e {
                if s.done == 1 {
                    cancel.cancel();
                }
                steps.push(s);
            }
        })
        .unwrap();
    let art = tree(cache.path())
        .into_iter()
        .map(|(p, size, _)| (p, size))
        .collect();
    (report, steps, art)
}

/// Art fetched four games at a time is the same as one at a time, and
/// progress counts every game up to its total.
#[test]
fn enrich_on_four_threads_matches_one() {
    let (one, steps, art) = enrich_run(1, Cancel::default());
    assert!(one.art_saved >= 3 && one.errors.is_empty(), "{one:?}");
    let last = steps.last().unwrap();
    assert_eq!((last.done, last.total), (one.games as u64, one.games as u64));
    let (four, _, art4) = enrich_run(4, Cancel::default());
    assert_eq!(
        (four.art_saved, four.art_missing),
        (one.art_saved, one.art_missing)
    );
    assert_eq!(art4, art);
}

/// Enrich asked to stop keeps what it saved and says it stopped.
#[test]
fn a_stopped_enrich_keeps_what_it_saved() {
    let (r, steps, art) = enrich_run(1, Cancel::new());
    assert!(r.stopped, "{r:?}");
    assert!(!art.is_empty());
    assert!(
        steps.iter().all(|s| s.done < s.total || s.total == 0),
        "{steps:?}"
    );
}

/// Opening the library writes nothing once it is up to date, so a second
/// connection (the app's network jobs) opens at once while a scan holds
/// its write transaction, instead of waiting and failing as "locked".
#[test]
fn opening_never_waits_for_a_running_scan() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("lib.db");
    drop(Library::open(&db).unwrap());
    let holder = rusqlite::Connection::open(&db).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let start = std::time::Instant::now();
    let lib = Library::open(&db).unwrap();
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
    assert!(lib.library_view().is_ok());
    holder.execute_batch("ROLLBACK").unwrap();
}
