use std::io::Read;

use super::*;

/// A small GameCube image laid out as a real one: header, apploader, DOL,
/// FST with two files, and each 32 KiB block's padding from its own seed.
fn gamecube_image(size: usize) -> Vec<u8> {
    let mut img = vec![0u8; size];
    let seed = |block: usize| -> [u32; junk::SEED_WORDS] {
        std::array::from_fn(|i| (block as u32 * 7919 + i as u32).wrapping_mul(0x9E37_79B9))
    };
    // Padding everywhere first, then the data over it.
    for block in 0..size.div_ceil(junk::BLOCK) {
        let from = block * junk::BLOCK;
        let len = junk::BLOCK.min(size - from);
        img[from..from + len].copy_from_slice(&junk::generate(&seed(block), 0, len));
    }
    let put = |img: &mut Vec<u8>, at: usize, v: u32| img[at..at + 4].copy_from_slice(&v.to_be_bytes());
    img[..0x2460].fill(0);
    img[..6].copy_from_slice(b"GALE01");
    img[0x1C..0x20].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]);
    let (dol, fst) = (0x10000usize, 0x30000usize);
    put(&mut img, 0x420, dol as u32);
    put(&mut img, 0x424, fst as u32);
    put(&mut img, 0x428, 36);
    put(&mut img, 0x2454, 0x1000); // apploader 0x20 + 0x1000
    img[0x2460..0x2460 + 0x1000].fill(0xAA);
    // DOL: one text section of 0x5000 bytes at 0x100.
    img[dol..dol + 0x100].fill(0);
    put(&mut img, dol, 0x100);
    put(&mut img, dol + 0x90, 0x5000);
    img[dol + 0x100..dol + 0x5100].fill(0x5D);
    // FST: root (3 entries) and two files, one ending mid-block.
    img[fst..fst + 36].fill(0);
    img[fst] = 1;
    put(&mut img, fst + 8, 3);
    put(&mut img, fst + 16, 0x40000);
    put(&mut img, fst + 20, 0x9876);
    put(&mut img, fst + 28, 0x60004);
    put(&mut img, fst + 32, 0x20);
    for (at, len, v) in [(0x40000usize, 0x9876usize, 0x11u8), (0x60004, 0x20, 0x22)] {
        for (i, b) in img[at..at + len].iter_mut().enumerate() {
            *b = v ^ (i % 251) as u8;
        }
    }
    img
}

#[test]
fn a_gamecube_disc_round_trips_and_its_padding_packs() {
    let img = gamecube_image(0xC0000 + 0x1234);
    let mut out = std::io::Cursor::new(Vec::new());
    write::write(|| Ok(&img[..]), img.len() as u64, 2, &mut out).unwrap();
    let file = out.into_inner();
    assert!(
        file.len() < img.len() / 4,
        "padding packs: {} of {}",
        file.len(),
        img.len()
    );
    let rvz = Rvz::open(std::io::Cursor::new(&file)).unwrap().unwrap();
    assert_eq!(rvz.iso_size, img.len() as u64);
    let mut back = Vec::new();
    rvz.iso().unwrap().read_to_end(&mut back).unwrap();
    assert!(back == img, "the image reads back byte for byte");
    // Not an RVZ: nothing.
    assert!(Rvz::open(std::io::Cursor::new(&img)).unwrap().is_none());
}

#[test]
fn the_layout_comes_from_the_header_and_fst() {
    let img = gamecube_image(0xC0000);
    let ranges = gc::data_ranges(&mut &img[..], img.len() as u64).unwrap();
    assert_eq!(
        ranges,
        [
            (0, 0x3460),
            (0x10000, 0x15100),
            (0x30000, 0x30024),
            (0x40000, 0x49876),
            (0x60004, 0x60024)
        ]
    );
}
