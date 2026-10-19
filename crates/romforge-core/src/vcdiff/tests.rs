use super::*;

#[test]
fn integers_round_trip() {
    for v in [0u64, 1, 127, 128, 300, 16384, u32::MAX as u64] {
        let mut b = Vec::new();
        write_int(v, &mut b);
        let mut at = 0;
        assert_eq!(read_int(&b, &mut at).unwrap(), v);
        assert_eq!(at, b.len());
    }
}

/// The source and target `fixtures/patches/*.xdelta` were made from,
/// by xdelta3 (`-S djw -W 16384`, and `-S fgk` with an xdelta3 built
/// with `SECONDARY_FGK`).
fn vectors() -> (Vec<u8>, Vec<u8>) {
    let mut x: u32 = 1;
    let src: Vec<u8> = (0..65536)
        .map(|_| {
            x = x.wrapping_mul(1103515245).wrapping_add(12345) & 0x7fff_ffff;
            (x >> 16) as u8
        })
        .collect();
    let text = b"the quick brown fox jumps over the lazy dog, said the translator. ";
    let tgt = (0..65536usize)
        .map(|i| {
            if (i / 4096) % 2 == 0 {
                src[i]
            } else {
                text[i % text.len()] ^ u8::from((i / text.len()).is_multiple_of(5))
            }
        })
        .collect();
    (src, tgt)
}

fn fixture(name: &str) -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/patches")
        .join(name);
    std::fs::read(p).unwrap()
}

#[test]
fn djw_and_fgk_patches_decode() {
    let (src, tgt) = vectors();
    for name in ["djw.xdelta", "fgk.xdelta"] {
        let patch = fixture(name);
        let plain = plain(&patch).unwrap();
        assert!(matches!(plain, Cow::Owned(_)), "{name}");
        let out = oxidelta::vcdiff::decode_memory(&plain, &src).unwrap();
        assert!(out == tgt, "{name}");
        // Through the patcher's own entry point as well.
        assert!(
            crate::patch::apply(&patch, &src).as_deref() == Some(&tgt[..]),
            "{name}"
        );
    }
}

#[test]
fn damaged_secondary_data_is_an_error() {
    let mut patch = fixture("djw.xdelta");
    let n = patch.len();
    patch.truncate(n - 40);
    assert!(plain(&patch).is_err());
}
