//! How many copies of each save the Settings keep.

use romforge_core::Library;

#[test]
fn the_copies_kept_default_to_five_and_stay_between_one_and_fifty() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("library.db")).unwrap();
    let mut s = lib.settings().unwrap();
    assert_eq!(s.save_copies, 5);
    s.save_copies = 12;
    assert_eq!(lib.save_settings(&s).unwrap().save_copies, 12);
    assert_eq!(lib.settings().unwrap().save_copies, 12);
    for bad in [0, 51] {
        s.save_copies = bad;
        assert!(lib.save_settings(&s).is_err(), "{bad} copies");
    }
}
