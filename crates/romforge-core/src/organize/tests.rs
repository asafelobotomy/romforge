use super::fs::{copy_verify_delete, rename_noreplace};
use super::*;

fn regions() -> Vec<String> {
    DEFAULT_REGIONS.iter().map(|s| s.to_string()).collect()
}

#[test]
fn titles_group_without_tags() {
    assert_eq!(base_title("Super Mario World (USA)"), "super mario world");
    assert_eq!(
        base_title("Super Mario World (Europe) (Rev 1) [!]"),
        "super mario world"
    );
    assert_ne!(base_title("Super Mario World 2 (USA)"), "super mario world");
}

#[test]
fn rank_prefers_release_region_then_revision() {
    let r = regions();
    let best = |names: &[&'static str]| -> &'static str {
        names.iter().copied().max_by_key(|n| rank(n, &r)).unwrap()
    };
    assert_eq!(best(&["X (Japan)", "X (USA)", "X (Europe)"]), "X (USA)");
    assert_eq!(
        best(&["X (USA)", "X (USA) (Rev 2)", "X (USA) (Rev 1)"]),
        "X (USA) (Rev 2)"
    );
    assert_eq!(best(&["X (USA) (Beta)", "X (Japan)"]), "X (Japan)");
    assert_eq!(best(&["X (USA, Europe)", "X (Europe)"]), "X (USA, Europe)");
    assert_eq!(best(&["X (Brazil)", "X (Japan)"]), "X (Japan)");
}

#[test]
fn renames_never_replace_and_links_stay() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    std::fs::write(&a, b"a").unwrap();
    std::fs::write(&b, b"b").unwrap();
    let e = rename_noreplace(&a, &b).unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&b).unwrap(), b"b");
    let c = dir.path().join("c");
    rename_noreplace(&a, &c).unwrap();
    assert!(!a.exists() && c.exists());

    #[cfg(unix)]
    {
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&c, &link).unwrap();
        assert!(move_file(&link, &dir.path().join("moved")).is_err());
        assert!(link.symlink_metadata().is_ok() && c.exists());
    }
}

#[test]
fn copy_verify_delete_moves_and_never_clobbers() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.bin"), dir.path().join("sub/b.bin"));
    std::fs::write(&a, vec![5u8; 100_000]).unwrap();
    std::fs::create_dir_all(b.parent().unwrap()).unwrap();
    let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    std::fs::File::options()
        .write(true)
        .open(&a)
        .unwrap()
        .set_modified(old)
        .unwrap();
    assert_eq!(copy_verify_delete(&a, &b, 100_000).unwrap(), 100_000);
    assert_eq!(
        std::fs::metadata(&b).unwrap().modified().unwrap(),
        old,
        "mtime kept"
    );
    assert!(!a.exists());
    assert_eq!(std::fs::read(&b).unwrap(), vec![5u8; 100_000]);
    let leftovers: Vec<_> = std::fs::read_dir(b.parent().unwrap()).unwrap().collect();
    assert_eq!(leftovers.len(), 1, "no .romforge-part left behind");

    std::fs::write(&a, b"new").unwrap();
    assert!(copy_verify_delete(&a, &b, 3).is_err(), "b exists");
    assert!(a.exists(), "the original stays when the move fails");
    assert!(move_file(&a, &b).is_err());
    assert_eq!(std::fs::read(&b).unwrap().len(), 100_000);
}

#[test]
fn sanitize_names() {
    assert_eq!(sanitize("A/B: C?"), "A-B- C-");
    assert_eq!(sanitize(" .. "), "_");
    assert_eq!(sanitize("Game."), "Game");
}

#[test]
fn sanitize_avoids_windows_reserved_names_and_long_ones() {
    assert_eq!(sanitize("CON"), "CON_");
    assert_eq!(sanitize("nul.txt"), "nul_.txt");
    assert_eq!(sanitize("COM1"), "COM1_");
    assert_eq!(sanitize("COM0"), "COM0");
    assert_eq!(sanitize("Contra (USA)"), "Contra (USA)");
    assert_eq!(sanitize("a: b?"), "a- b-");
    let long = "é".repeat(150);
    let s = sanitize(&long);
    assert!(s.len() <= 200 && s.chars().all(|c| c == 'é'), "{}", s.len());
}
