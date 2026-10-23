use super::*;

#[test]
fn boxes_by_platform_and_region() {
    let k = |key: &str| by_key(key).unwrap();
    let codes = |c: &[&str]| c.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(BoxRegion::of(&codes(&["US"])), BoxRegion::NorthAmerica);
    assert_eq!(BoxRegion::of(&codes(&["JP"])), BoxRegion::Japan);
    assert_eq!(BoxRegion::of(&codes(&["EU"])), BoxRegion::Europe);
    assert_eq!(BoxRegion::of(&codes(&["US", "EU"])), BoxRegion::NorthAmerica);
    assert_eq!(BoxRegion::of(&[]), BoxRegion::NorthAmerica);
    assert!(box_aspect(k("SNES"), BoxRegion::NorthAmerica) > 1.3, "landscape");
    assert!(box_aspect(k("Genesis"), BoxRegion::Europe) < 0.75, "portrait");
    assert!(
        box_aspect(k("N64"), BoxRegion::Japan) < 1.0,
        "Japan's N64 boxes stand up"
    );
    assert!(
        box_aspect(k("NES"), BoxRegion::Japan) > 1.3,
        "the Famicom's lie down"
    );
    assert_eq!(box_aspect(k("PlayStation"), BoxRegion::Japan), 1.0);
    assert_eq!(
        box_aspect(k("PlayStation 2"), BoxRegion::NorthAmerica),
        0.71,
        "a DVD case by rule"
    );
    assert_eq!(box_aspect(k("DOS"), BoxRegion::NorthAmerica), 0.8, "a big box");
}

#[test]
fn box_sizes_are_real_and_keep_their_shape() {
    let k = |key: &str| by_key(key).unwrap();
    assert_eq!(
        box_size(k("PlayStation 2"), BoxRegion::NorthAmerica),
        [135, 190, 14],
        "a DVD case by rule"
    );
    assert_eq!(box_size(k("DOS"), BoxRegion::Europe)[2], 50, "a big box is deep");
    assert_eq!(
        box_size(k("SNES"), BoxRegion::Japan),
        [192, 106, 30],
        "a listed size"
    );
    // Every platform's box, listed or worked out, is a box one could hold.
    for s in SYSTEMS {
        for region in [BoxRegion::NorthAmerica, BoxRegion::Europe, BoxRegion::Japan] {
            let [w, h, d] = box_size(s, region);
            assert!(
                (60..=300).contains(&w) && (60..=300).contains(&h) && (8..=60).contains(&d),
                "{} {region:?}: {w}x{h}x{d}",
                s.key
            );
        }
    }
    for (key, _) in BOX_SIZES {
        assert!(by_key(key).is_some(), "{key} isn't a platform");
    }
}
