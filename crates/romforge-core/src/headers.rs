//! The one table of copier headers. Nothing else in romforge detects headers.
//!
//! Every file is hashed twice: raw, and with its header stripped when one of
//! these rules fires. Matching accepts either, so a false positive (e.g. the
//! size-only SNES rule) costs nothing but an extra hash.

#[derive(Debug, Clone, Copy)]
pub struct HeaderRule {
    /// Stable id stored in the database.
    pub kind: &'static str,
    /// Bytes to skip.
    pub len: usize,
    detect: fn(prefix: &[u8], size: u64) -> bool,
}

/// Bytes of a file's start that must be read before calling [`detect`].
pub const PREFIX_LEN: usize = 128;

pub const HEADERS: &[HeaderRule] = &[
    // iNES and NES 2.0 share the magic and the 16-byte length.
    HeaderRule {
        kind: "ines",
        len: 16,
        detect: |p, _| p.starts_with(b"NES\x1a"),
    },
    HeaderRule {
        kind: "fds",
        len: 16,
        detect: |p, _| p.starts_with(b"FDS\x1a"),
    },
    HeaderRule {
        kind: "lynx",
        len: 64,
        detect: |p, _| p.starts_with(b"LYNX"),
    },
    HeaderRule {
        kind: "a7800",
        len: 128,
        detect: |p, _| p.get(1..10) == Some(b"ATARI7800"),
    },
    // SMC/SWC/FIG copier header: ROM data is always a multiple of 1 KiB.
    HeaderRule {
        kind: "snes-copier",
        len: 512,
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
