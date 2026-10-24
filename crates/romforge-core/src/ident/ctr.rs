//! Milestone 24: the 3DS's own headers, plain fields only (as 3dbrew
//! documents them: NCSD, NCCH, CIA). A card image (`.3ds`/`.cci`) is an
//! NCSD whose first partition is an NCCH; a `.cxi` is that NCCH alone; a
//! CIA holds its contents after its header, certificates, ticket and TMD,
//! each padded to 64 bytes (an encrypted CIA's NCCH can't be read, and
//! gives nothing). A Z3DS file is read by the scan as the file it holds.

use super::{GameId, code, code_region, id};

const MEDIA_UNIT: usize = 0x200;

fn u32_at(p: &[u8], i: usize) -> Option<usize> {
    Some(u32::from_le_bytes(p.get(i..i + 4)?.try_into().ok()?) as usize)
}

/// An NCCH header at `at`: its product code (`CTR-P-ABCE`) as the serial,
/// the region its game code's last letter gives.
fn ncch(p: &[u8], at: usize) -> Option<GameId> {
    if p.get(at + 0x100..at + 0x104)? != b"NCCH" {
        return None;
    }
    let field = p.get(at + 0x150..at + 0x160)?;
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    let product = std::str::from_utf8(&field[..end]).ok()?.trim();
    // `CTR-P-ABCE` (or `KTR-` for New 3DS games): the game code is the
    // last four letters.
    let (_, game) = product.rsplit_once('-')?;
    code(game.as_bytes()).filter(|g| g.len() == 4)?;
    let region = game.as_bytes().get(3).copied().and_then(code_region);
    Some(id("3ds", Some(product.to_owned()), None, region))
}

pub(super) fn ctr(p: &[u8]) -> Option<GameId> {
    // A card image: NCSD, the first partition's offset in media units.
    if p.get(0x100..0x104) == Some(b"NCSD") {
        return ncch(p, u32_at(p, 0x120)? * MEDIA_UNIT);
    }
    // A CXI: the NCCH itself.
    if let Some(found) = ncch(p, 0) {
        return Some(found);
    }
    // A CIA: header size 0x2020, type 0, then the parts in order.
    if u32_at(p, 0)? == 0x2020 && p.get(4..6)? == [0, 0] {
        let align = |n: usize| n.div_ceil(64) * 64;
        let content =
            align(0x2020) + align(u32_at(p, 0x08)?) + align(u32_at(p, 0x0C)?) + align(u32_at(p, 0x10)?);
        return ncch(p, content);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An NCCH header with `product` at 0x150.
    fn ncch_header(product: &str) -> Vec<u8> {
        let mut h = vec![0u8; 0x200];
        h[0x100..0x104].copy_from_slice(b"NCCH");
        h[0x150..0x150 + product.len()].copy_from_slice(product.as_bytes());
        h
    }

    #[test]
    fn card_cxi_and_cia_headers_give_the_product_code() {
        // A card image: NCSD with partition 0 at 0x20 media units (0x4000).
        let mut cci = vec![0u8; 0x4000];
        cci[0x100..0x104].copy_from_slice(b"NCSD");
        cci[0x120..0x124].copy_from_slice(&0x20u32.to_le_bytes());
        cci.extend(ncch_header("CTR-P-AREE"));
        let id = ctr(&cci).unwrap();
        assert_eq!(
            (id.kind, id.serial.as_deref(), id.region.as_deref()),
            ("3ds", Some("CTR-P-AREE"), Some("USA"))
        );
        assert_eq!(
            ctr(&ncch_header("KTR-P-BCYJ")).unwrap().region.as_deref(),
            Some("Japan"),
            "a New 3DS game"
        );
        // A CIA: 0x2020 header, then certificates (0xA00), ticket (0x350),
        // TMD (0xB34), each padded to 64 bytes.
        let mut cia = vec![0u8; 0x2040 + 0xA00 + 0x380 + 0xB40];
        cia[0..4].copy_from_slice(&0x2020u32.to_le_bytes());
        cia[8..12].copy_from_slice(&0xA00u32.to_le_bytes());
        cia[12..16].copy_from_slice(&0x350u32.to_le_bytes());
        cia[16..20].copy_from_slice(&0xB34u32.to_le_bytes());
        cia.extend(ncch_header("CTR-P-BP4P"));
        assert_eq!(ctr(&cia).unwrap().serial.as_deref(), Some("CTR-P-BP4P"));
        // An encrypted CIA's content, or anything else: nothing.
        let mut enc = cia.clone();
        let at = enc.len() - 0x200 + 0x100;
        enc[at..at + 4].copy_from_slice(&[0x9a, 0x01, 0x7f, 0x00]);
        assert!(ctr(&enc).is_none());
        assert!(ctr(&[0u8; 0x400]).is_none());
        assert!(
            ctr(&ncch_header("CTR-P-AR")).is_none(),
            "a game code is four letters"
        );
    }
}
