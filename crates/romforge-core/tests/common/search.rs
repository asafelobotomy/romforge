//! Helpers from the `search` tests.

/// A LoROM SNES image of `len` bytes with a valid header.
pub fn snes_rom(title: &[u8; 21], code: &[u8; 4], region: u8, len: usize, fill: u8) -> Vec<u8> {
    let mut rom = vec![fill; len];
    let h = 0x7FC0;
    rom[h..h + 21].copy_from_slice(title);
    rom[h - 0x0E..h - 0x0A].copy_from_slice(code);
    rom[h + 0x19] = region;
    rom[h + 0x1A] = 0x33;
    rom[h + 0x1C..h + 0x20].copy_from_slice(&[0x1A, 0x93, 0xE5, 0x6C]);
    rom
}

/// BPS's variable-length number, and a BPS patch with just its header and
/// footer (enough for romforge, which only reads those).
pub fn bps(source: &[u8], target: &[u8]) -> Vec<u8> {
    fn number(mut n: u64, out: &mut Vec<u8>) {
        loop {
            let x = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(0x80 | x);
                return;
            }
            out.push(x);
            n -= 1;
        }
    }
    let mut p = b"BPS1".to_vec();
    number(source.len() as u64, &mut p);
    number(target.len() as u64, &mut p);
    number(0, &mut p);
    let crc = |d: &[u8]| {
        u32::from_str_radix(
            &romforge_core::hash::hash_stream(d, d.len() as u64)
                .unwrap()
                .raw
                .crc32,
            16,
        )
        .unwrap()
    };
    p.extend_from_slice(&crc(source).to_le_bytes());
    p.extend_from_slice(&crc(target).to_le_bytes());
    let own = crc(&p);
    p.extend_from_slice(&own.to_le_bytes());
    p
}
