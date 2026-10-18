//! The one table of copier headers and byte orders. Nothing else in romforge
//! detects them.
//!
//! Every file is hashed twice: raw, and normalized (header stripped, bytes
//! put back in the DAT's order) when one of these rules fires. Matching
//! accepts either, so a false positive (e.g. the size-only SNES rule) costs
//! nothing but an extra hash. Files are never rewritten for this.

#[derive(Debug, Clone, Copy)]
pub struct HeaderRule {
    /// Stable id stored in the database.
    pub kind: &'static str,
    /// Bytes to skip.
    pub len: usize,
    /// How the bytes after the header are put back in order.
    pub reorder: Reorder,
    detect: fn(prefix: &[u8], size: u64) -> bool,
}

/// Byte orders that dumps come in besides the DAT's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reorder {
    None,
    /// Each 16-bit word swapped (N64 `.v64`).
    Swap16,
    /// Each 32-bit word reversed (N64 `.n64`).
    Swap32,
    /// Super Magic Drive interleave: in each 16 KiB block the first half
    /// holds the odd bytes, the second half the even ones (Mega Drive `.smd`).
    SmdBlocks,
}

impl Reorder {
    /// Bytes reordered as one piece.
    pub fn unit(self) -> usize {
        match self {
            Reorder::None => 1,
            Reorder::Swap16 => 2,
            Reorder::Swap32 => 4,
            Reorder::SmdBlocks => SMD_BLOCK,
        }
    }

    /// Reorder whole units of `data` (a multiple of [`unit`](Self::unit))
    /// into `out`.
    pub fn apply(self, data: &[u8], out: &mut Vec<u8>) {
        out.clear();
        match self {
            Reorder::None => out.extend_from_slice(data),
            Reorder::Swap16 => {
                for w in data.as_chunks::<2>().0 {
                    out.extend_from_slice(&[w[1], w[0]]);
                }
            }
            Reorder::Swap32 => {
                for w in data.as_chunks::<4>().0 {
                    out.extend_from_slice(&[w[3], w[2], w[1], w[0]]);
                }
            }
            Reorder::SmdBlocks => {
                for block in data.as_chunks::<SMD_BLOCK>().0 {
                    let (odd, even) = block.split_at(SMD_BLOCK / 2);
                    for (e, o) in even.iter().zip(odd) {
                        out.extend_from_slice(&[*e, *o]);
                    }
                }
            }
        }
    }
}

const SMD_BLOCK: usize = 16 * 1024;

/// The first word of an N64 ROM in each byte order (`.z64` is the DAT's).
const N64_V64: [u8; 4] = [0x37, 0x80, 0x40, 0x12];
const N64_N64: [u8; 4] = [0x40, 0x12, 0x37, 0x80];

/// Bytes of a file's start that must be read before calling [`detect`].
pub const PREFIX_LEN: usize = 128;

pub const HEADERS: &[HeaderRule] = &[
    // iNES and NES 2.0 share the magic and the 16-byte length.
    HeaderRule {
        kind: "ines",
        len: 16,
        reorder: Reorder::None,
        detect: |p, _| p.starts_with(b"NES\x1a"),
    },
    HeaderRule {
        kind: "fds",
        len: 16,
        reorder: Reorder::None,
        detect: |p, _| p.starts_with(b"FDS\x1a"),
    },
    HeaderRule {
        kind: "lynx",
        len: 64,
        reorder: Reorder::None,
        detect: |p, _| p.starts_with(b"LYNX"),
    },
    HeaderRule {
        kind: "a7800",
        len: 128,
        reorder: Reorder::None,
        detect: |p, _| p.get(1..10) == Some(b"ATARI7800"),
    },
    HeaderRule {
        kind: "n64-v64",
        len: 0,
        reorder: Reorder::Swap16,
        detect: |p, size| p.starts_with(&N64_V64) && size % 4 == 0,
    },
    HeaderRule {
        kind: "n64-n64",
        len: 0,
        reorder: Reorder::Swap32,
        detect: |p, size| p.starts_with(&N64_N64) && size % 4 == 0,
    },
    // Super Magic Drive: a 512-byte header (bytes 8-9 are AA BB, byte 1 is
    // 3), then interleaved 16 KiB blocks. Before the SNES rule, which the
    // size would also fire.
    HeaderRule {
        kind: "smd",
        len: 512,
        reorder: Reorder::SmdBlocks,
        detect: |p, size| {
            p.get(1) == Some(&3) && p.get(8..10) == Some(&[0xAA, 0xBB]) && size % SMD_BLOCK as u64 == 512
        },
    },
    // SMC/SWC/FIG copier header: ROM data is always a multiple of 1 KiB.
    // Also strips PC Engine copier headers (their data is a multiple of
    // 128 KiB), so those need no rule of their own.
    HeaderRule {
        kind: "snes-copier",
        len: 512,
        reorder: Reorder::None,
        detect: |_, size| size % 1024 == 512,
    },
];

/// Find the header rule for a file, given its first bytes (at least
/// [`PREFIX_LEN`] when the file is that long) and its total size.
pub fn detect(prefix: &[u8], size: u64) -> Option<&'static HeaderRule> {
    HEADERS
        .iter()
        .find(|h| size > h.len as u64 && (h.detect)(prefix, size))
}

/// Whether an iNES header is NES 2.0 (byte 7, bits 2-3 == 0b10).
pub fn is_nes2(prefix: &[u8]) -> bool {
    prefix.starts_with(b"NES\x1a") && prefix.get(7).is_some_and(|b| b & 0x0c == 0x08)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(prefix: &[u8], size: u64) -> Option<&'static str> {
        detect(prefix, size).map(|h| h.kind)
    }

    #[test]
    fn magic_headers() {
        let mut ines = [0u8; 128];
        ines[..4].copy_from_slice(b"NES\x1a");
        assert_eq!(kind(&ines, 16 + 1024), Some("ines"));
        assert!(!is_nes2(&ines));
        ines[7] = 0x08;
        assert!(is_nes2(&ines));

        let mut fds = [0u8; 128];
        fds[..4].copy_from_slice(b"FDS\x1a");
        assert_eq!(kind(&fds, 16 + 1024), Some("fds"));

        let mut lynx = [0u8; 128];
        lynx[..4].copy_from_slice(b"LYNX");
        assert_eq!(kind(&lynx, 64 + 1024), Some("lynx"));

        let mut a78 = [0u8; 128];
        a78[1..10].copy_from_slice(b"ATARI7800");
        assert_eq!(kind(&a78, 128 + 1024), Some("a7800"));
    }

    #[test]
    fn byte_orders() {
        let mut p = [0u8; 128];
        p[..4].copy_from_slice(&N64_V64);
        assert_eq!(kind(&p, 8 * 1024 * 1024), Some("n64-v64"));
        p[..4].copy_from_slice(&N64_N64);
        assert_eq!(kind(&p, 8 * 1024 * 1024), Some("n64-n64"));
        p[..4].copy_from_slice(&[0x80, 0x37, 0x12, 0x40]);
        assert_eq!(kind(&p, 8 * 1024 * 1024), None, "z64 needs nothing");

        let mut smd = [0u8; 128];
        smd[1] = 3;
        smd[8..10].copy_from_slice(&[0xAA, 0xBB]);
        assert_eq!(kind(&smd, 512 + 32 * 1024), Some("smd"));
        // Without the magic the size still strips it as a copier header.
        assert_eq!(kind(&[0u8; 128], 512 + 32 * 1024), Some("snes-copier"));
    }

    #[test]
    fn pc_engine_headers_strip_by_size() {
        assert_eq!(kind(&[0u8; 128], 512 + 256 * 1024), Some("snes-copier"));
    }

    #[test]
    fn lynx_needs_magic() {
        assert_eq!(kind(&[0u8; 128], 64 + 1024), None);
    }

    #[test]
    fn snes_rule_is_size_mod_1024() {
        assert_eq!(kind(&[0u8; 128], 512 + 4096), Some("snes-copier"));
        assert_eq!(kind(&[0u8; 128], 4096), None);
        // The old wrong rule ("rest is a power of two") would miss this.
        assert_eq!(kind(&[0u8; 128], 512 + 3 * 1024), Some("snes-copier"));
        assert_eq!(kind(&[0u8; 128], 512), None);
    }

    #[test]
    fn magic_wins_over_size() {
        let mut ines = [0u8; 128];
        ines[..4].copy_from_slice(b"NES\x1a");
        // 16 + 1520 = 1536, which is also 512 mod 1024.
        assert_eq!(kind(&ines, 1536), Some("ines"));
    }
}
