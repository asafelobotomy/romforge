use super::*;

fn names(list: &[(&str, u64)]) -> Names {
    list.iter().map(|(n, s)| ((*n).to_owned(), *s)).collect()
}

/// A v5 index file's start: "RNAM" and a size, XORed with 0x69.
fn v5_head() -> [u8; 8] {
    let mut h = *b"RNAM\0\0\0\x20";
    h.iter_mut().for_each(|b| *b ^= 0x69);
    h
}

fn engine(n: &Names, head: Option<[u8; 8]>) -> Option<(&'static str, Option<String>)> {
    engine_of(n, &|_| false, &mut |_| head)
}

#[test]
fn scumm_index_files_name_the_game() {
    let dott = names(&[
        ("tentacle.000", 4000),
        ("tentacle.001", 9_000_000),
        ("dott.exe", 100),
    ]);
    assert_eq!(
        engine(&dott, Some(v5_head())),
        Some(("scumm", Some("tentacle".into())))
    );
    // v7 and v8 aren't XORed.
    let ft = names(&[("ft.la0", 5000), ("ft.la1", 9_000_000)]);
    assert_eq!(
        engine(&ft, Some(*b"RNAM\0\0\0\x10")),
        Some(("scumm", Some("ft".into())))
    );
    // Other bytes: a split archive's parts, not a game.
    assert_eq!(engine(&dott, Some(*b"7z\xbc\xaf\x27\x1c\0\x04")), None);
    // An index without the file beside it.
    assert_eq!(engine(&names(&[("tentacle.000", 4000)]), Some(v5_head())), None);
    // v1 to v4: numbered rooms.
    let loom = names(&[("000.lfl", 2000), ("901.lfl", 3000), ("disk01.lec", 90_000)]);
    assert_eq!(engine(&loom, None), Some(("scumm", None)));
    assert_eq!(engine(&names(&[("00.lfl", 2000)]), None), None);
}

#[test]
fn sci_and_sky_by_their_files() {
    let kq = names(&[
        ("resource.map", 3000),
        ("resource.000", 400_000),
        ("sierra.exe", 100),
    ]);
    assert_eq!(engine(&kq, None), Some(("sci", None)));
    let gk = names(&[("resmap.001", 3000), ("ressci.001", 400_000)]);
    assert_eq!(engine(&gk, None), Some(("sci", None)));
    // SCI's numbered files are never read as SCUMM's.
    assert_eq!(
        engine(
            &names(&[("resource.000", 1), ("resource.001", 1)]),
            Some(v5_head())
        ),
        None
    );
    let sky = names(&[("sky.dnr", 80), ("sky.dsk", 9000)]);
    assert_eq!(engine(&sky, None), Some(("sky", Some("sky".into()))));
    // Files ScummVM's list names are its to decide.
    assert_eq!(engine_of(&sky, &|n| n == "sky.dnr", &mut |_| None), None);
}
