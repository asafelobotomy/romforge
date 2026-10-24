//! The padding ("junk") of GameCube and Wii discs, as RVZ packs it
//! (Dolphin's `docs/WiaAndRvz.md`, "PRNG algorithm"): a lagged Fibonacci
//! generator (xor, j = 32, k = 521) seeded with 17 words, each 32 KiB block
//! of a disc from its own seed, the stream advanced by the offset within
//! the block.
//!
//! Recovering a seed from padding is romforge's own: every step of the
//! generator is shifts and xors, so each output bit is the parity of some
//! of the 544 seed bits. Those masks are worked out once for a whole
//! block; a run of padding at any offset then gives one equation per bit
//! it shows, solved by Gaussian elimination over GF(2). Two seed bits never
//! reach the output, so they are left 0. A seed is used only once the run,
//! regenerated from it, equals the data byte for byte.

use std::sync::OnceLock;

pub const SEED_WORDS: usize = 17;
const K: usize = 521;
const J: usize = 32;
pub const BLOCK: usize = 0x8000;
const WORDS: usize = SEED_WORDS * 32 / 64; // 544 bits = 8.5 u64
const W: usize = WORDS + 1;
/// Two seed bits (word 0's bits 16 and 17) never reach the output: no
/// padding tells more than 542 of the 544.
const MAX_RANK: usize = SEED_WORDS * 32 - 2;

/// The generator's state and where its output is.
pub struct Lfg {
    buf: [u32; K],
    /// The next byte of output, within `buf` (0..K*4).
    at: usize,
}

fn advance(buf: &mut [u32; K]) {
    for i in 0..J {
        buf[i] ^= buf[i + K - J];
    }
    for i in J..K {
        buf[i] ^= buf[i - J];
    }
}

/// A word's four bytes as the generator outputs them (the second is `>> 18`,
/// not 16).
fn bytes_of(w: u32) -> [u8; 4] {
    [(w >> 24) as u8, (w >> 18) as u8, (w >> 8) as u8, w as u8]
}

impl Lfg {
    /// Seeded, ready to output from the start of a block.
    pub fn new(seed: &[u32; SEED_WORDS]) -> Lfg {
        let mut buf = [0u32; K];
        buf[..SEED_WORDS].copy_from_slice(seed);
        for i in SEED_WORDS..K {
            buf[i] = (buf[i - 17] << 23) ^ (buf[i - 16] >> 9) ^ buf[i - 1];
        }
        for _ in 0..4 {
            advance(&mut buf);
        }
        Lfg { buf, at: 0 }
    }

    /// Pass over `n` bytes of output.
    pub fn skip(&mut self, mut n: usize) {
        while n > 0 {
            let here = (K * 4 - self.at).min(n);
            self.at += here;
            n -= here;
            if self.at == K * 4 {
                advance(&mut self.buf);
                self.at = 0;
            }
        }
    }

    /// Fill `out` with the next bytes.
    pub fn fill(&mut self, out: &mut [u8]) {
        for b in out {
            *b = bytes_of(self.buf[self.at / 4])[self.at % 4];
            self.at += 1;
            if self.at == K * 4 {
                advance(&mut self.buf);
                self.at = 0;
            }
        }
    }
}

/// The padding a seed gives at `offset` (within its block) for `len` bytes.
pub fn generate(seed: &[u32; SEED_WORDS], offset: usize, len: usize) -> Vec<u8> {
    let mut g = Lfg::new(seed);
    g.skip(offset);
    let mut out = vec![0u8; len];
    g.fill(&mut out);
    out
}

/// A GF(2) mask over the seed bits (bit `32 * w + b` is word `w`'s bit `b`).
type Mask = [u64; W];

fn xor(a: &mut Mask, b: &Mask) {
    for (x, y) in a.iter_mut().zip(b) {
        *x ^= y;
    }
}

/// For each output byte of a block and each of its 8 bits, which seed bits
/// it is the parity of. 32 KiB × 8 masks of 72 bytes: about 18 MB, made
/// once.
fn masks() -> &'static [Mask] {
    static MASKS: OnceLock<Vec<Mask>> = OnceLock::new();
    MASKS.get_or_init(|| {
        // The state symbolically: word i's bit b as a mask.
        let mut buf: Vec<[Mask; 32]> = vec![[[0u64; W]; 32]; K];
        for (w, word) in buf.iter_mut().enumerate().take(SEED_WORDS) {
            for (b, m) in word.iter_mut().enumerate() {
                let bit = w * 32 + b;
                m[bit / 64] |= 1 << (bit % 64);
            }
        }
        let shifted = |src: &[Mask; 32], left: i32| -> [Mask; 32] {
            let mut out = [[0u64; W]; 32];
            for (b, o) in out.iter_mut().enumerate() {
                let from = b as i32 - left;
                if (0..32).contains(&from) {
                    *o = src[from as usize];
                }
            }
            out
        };
        for i in SEED_WORDS..K {
            let mut w = shifted(&buf[i - 17], 23);
            let r = shifted(&buf[i - 16], -9);
            let prev = buf[i - 1];
            for b in 0..32 {
                xor(&mut w[b], &r[b]);
                xor(&mut w[b], &prev[b]);
            }
            buf[i] = w;
        }
        let step = |buf: &mut Vec<[Mask; 32]>| {
            for i in 0..J {
                let other = buf[i + K - J];
                for b in 0..32 {
                    xor(&mut buf[i][b], &other[b]);
                }
            }
            for i in J..K {
                let other = buf[i - J];
                for b in 0..32 {
                    xor(&mut buf[i][b], &other[b]);
                }
            }
        };
        for _ in 0..4 {
            step(&mut buf);
        }
        // Which word bit each output byte's bits come from: byte 0 is bits
        // 24..31, byte 1 bits 18..25, byte 2 bits 8..15, byte 3 bits 0..7.
        const LOW: [usize; 4] = [24, 18, 8, 0];
        let mut out = Vec::with_capacity(BLOCK * 8);
        let mut word = 0;
        for byte in 0..BLOCK {
            let w = (byte / 4) - word * K;
            if w == K {
                step(&mut buf);
                word += 1;
            }
            let w = (byte / 4) - word * K;
            for bit in 0..8 {
                out.push(buf[w][LOW[byte % 4] + bit]);
            }
        }
        out
    })
}

/// Bytes of a block's start the precomputed solution reads.
const FAST_BYTES: usize = 640;
const FAST_EQ: usize = FAST_BYTES * 8 / 64;

/// For padding from a block's start, the common case: each seed bit as
/// the parity of a set of the first `FAST_BYTES` bytes' bits, worked out
/// once by the same elimination with each row's equations tracked.
fn from_start() -> &'static [(usize, [u64; FAST_EQ])] {
    static INV: OnceLock<Vec<(usize, [u64; FAST_EQ])>> = OnceLock::new();
    INV.get_or_init(|| {
        let masks = masks();
        let mut pivots: Vec<Option<(Mask, [u64; FAST_EQ])>> = vec![None; SEED_WORDS * 32];
        for eq in 0..FAST_BYTES * 8 {
            let mut row = masks[eq];
            let mut combo = [0u64; FAST_EQ];
            combo[eq / 64] |= 1 << (eq % 64);
            for col in 0..SEED_WORDS * 32 {
                if row[col / 64] >> (col % 64) & 1 == 1
                    && let Some((p, c)) = &pivots[col]
                {
                    xor(&mut row, p);
                    combo.iter_mut().zip(c).for_each(|(a, b)| *a ^= b);
                }
            }
            let Some(col) = (0..SEED_WORDS * 32).find(|c| row[c / 64] >> (c % 64) & 1 == 1) else {
                continue;
            };
            for (p, c) in pivots.iter_mut().flatten() {
                if p[col / 64] >> (col % 64) & 1 == 1 {
                    xor(p, &row);
                    c.iter_mut().zip(&combo).for_each(|(a, b)| *a ^= b);
                }
            }
            pivots[col] = Some((row, combo));
        }
        pivots
            .into_iter()
            .enumerate()
            .filter_map(|(col, p)| p.map(|(_, c)| (col, c)))
            .collect()
    })
}

/// The seed whose padding, from `offset` within its block, is `data`, if
/// there is one. At least 72 bytes are needed; all of `data` is checked.
pub fn find_seed(data: &[u8], offset: usize) -> Option<[u32; SEED_WORDS]> {
    if data.len() < 72 || offset + data.len() > BLOCK || data.iter().all(|&b| b == 0) {
        return None;
    }
    if offset == 0 && data.len() >= FAST_BYTES {
        let mut seen = [0u64; FAST_EQ];
        for (i, &byte) in data[..FAST_BYTES].iter().enumerate() {
            seen[i / 8] |= u64::from(byte) << ((i % 8) * 8);
        }
        let mut seed = [0u32; SEED_WORDS];
        for (col, combo) in from_start() {
            let parity = combo
                .iter()
                .zip(&seen)
                .map(|(a, b)| (a & b).count_ones())
                .sum::<u32>()
                & 1;
            seed[col / 32] |= parity << (col % 32);
        }
        return (generate(&seed, 0, data.len()) == data).then_some(seed);
    }
    let masks = masks();
    // Pivots by column: fully reduced rows (mask, value).
    let mut pivots: Vec<Option<(Mask, bool)>> = vec![None; SEED_WORDS * 32];
    let mut rank = 0;
    'bytes: for (i, &byte) in data.iter().enumerate() {
        for bit in 0..8 {
            let mut row = masks[(offset + i) * 8 + bit];
            let mut value = byte >> bit & 1 == 1;
            for col in 0..SEED_WORDS * 32 {
                if row[col / 64] >> (col % 64) & 1 == 1
                    && let Some((p, v)) = &pivots[col]
                {
                    xor(&mut row, p);
                    value ^= v;
                }
            }
            let Some(col) = (0..SEED_WORDS * 32).find(|c| row[c / 64] >> (c % 64) & 1 == 1) else {
                if value {
                    return None; // 0 = 1: not this generator's output
                }
                continue;
            };
            // Keep every pivot reduced by the new one.
            for p in pivots.iter_mut().flatten() {
                if p.0[col / 64] >> (col % 64) & 1 == 1 {
                    xor(&mut p.0, &row);
                    p.1 ^= value;
                }
            }
            pivots[col] = Some((row, value));
            rank += 1;
            if rank == MAX_RANK {
                break 'bytes;
            }
        }
    }
    // Columns no equation fixed (always word 0's bits 16 and 17, which
    // never reach the output; more for a short run) are left 0: any seed
    // that regenerates the data is right, and that is checked below.
    let mut seed = [0u32; SEED_WORDS];
    for (col, p) in pivots.iter().enumerate() {
        if p.as_ref().is_some_and(|(_, v)| *v) {
            seed[col / 32] |= 1 << (col % 32);
        }
    }
    (generate(&seed, offset, data.len()) == data).then_some(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(n: u32) -> [u32; SEED_WORDS] {
        std::array::from_fn(|i| (i as u32 + 1).wrapping_mul(0x9E37_79B9) ^ n)
    }

    #[test]
    fn the_generator_follows_the_spec() {
        // Skipping and filling agree with one long fill.
        let s = seed(1);
        let whole = generate(&s, 0, 5000);
        assert_eq!(generate(&s, 1234, 100), whole[1234..1334]);
        assert_eq!(generate(&s, 2083, 9), whole[2083..2092], "across a buffer's end");
        // The second byte is the word's bits 18..25.
        let mut g = Lfg::new(&s);
        let mut four = [0u8; 4];
        g.fill(&mut four);
        let w = Lfg::new(&s).buf[0];
        assert_eq!(four, [(w >> 24) as u8, (w >> 18) as u8, (w >> 8) as u8, w as u8]);
    }

    #[test]
    fn a_seed_is_found_from_its_padding_anywhere_in_a_block() {
        for (n, offset, len) in [
            (2u32, 0usize, 600usize),
            (7, 0, BLOCK),
            (3, 4, 600),
            (4, 1001, 600),
            (5, 0x7F00, 256),
        ] {
            let s = seed(n);
            let data = generate(&s, offset, len);
            let found = find_seed(&data, offset).expect("a seed");
            assert_eq!(generate(&found, offset, data.len()), data, "offset {offset}");
        }
        // Not padding: nothing.
        let mut data = generate(&seed(6), 64, 300);
        data[200] ^= 1;
        assert!(find_seed(&data, 64).is_none());
        assert!(find_seed(&[0u8; 300], 0).is_none());
        assert!(find_seed(&[7u8; 50], 0).is_none(), "too short");
    }
}
