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

/// What a 3DS file is for Z3DS (milestone 23): the magic Azahar writes
/// for it, its frame size, and whether it is decrypted. Read from plain
/// fields only, never by decrypting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Z3dsForm {
    pub magic: [u8; 4],
    pub frame: usize,
    pub decrypted: bool,
}

/// An NCCH at `at` whose flags say NoCrypto (flags byte 7, bit 0x04).
fn plain_ncch(p: &[u8], at: usize) -> Option<bool> {
    if p.get(at + 0x100..at + 0x104)? != b"NCCH" {
        return None;
    }
    Some(p.get(at + 0x18F)? & 0x04 != 0)
}

/// Azahar 2126.2's frame sizes (`file_derived.h`): 256 KiB, a CIA 32 MiB.
const FRAME: usize = 256 * 1024;
const CIA_FRAME: usize = 32 * 1024 * 1024;

pub fn z3ds_form(p: &[u8]) -> Option<Z3dsForm> {
    let form = |magic: &[u8; 4], frame, decrypted| Z3dsForm {
        magic: *magic,
        frame,
        decrypted,
    };
    if p.get(..4)? == b"3DSX" {
        return Some(form(b"3DSX", FRAME, true));
    }
    if p.get(0x100..0x104) == Some(b"NCSD") {
        let first = u32_at(p, 0x120)? * MEDIA_UNIT;
        return Some(form(b"NCSD", FRAME, plain_ncch(p, first).unwrap_or(false)));
    }
    if let Some(plain) = plain_ncch(p, 0) {
        return Some(form(b"NCCH", FRAME, plain));
    }
    if u32_at(p, 0)? == 0x2020 && p.get(4..6)? == [0, 0] {
        let align = |n: usize| n.div_ceil(64) * 64;
        let tmd = align(0x2020) + align(u32_at(p, 0x08)?) + align(u32_at(p, 0x0C)?);
        let content = tmd + align(u32_at(p, 0x10)?);
        // The TMD's signature type says how long its signature is; its
        // header (0xC4) and 64 content info records (0x900) follow, then
        // content 0's record, whose type has bit 0x0001 when encrypted.
        let sig = u32::from_be_bytes(p.get(tmd..tmd + 4)?.try_into().ok()?);
        let sig_len = match sig {
            0x0001_0003 => 0x200 + 0x3C,
            0x0001_0004 => 0x100 + 0x3C,
            0x0001_0005 => 0x3C + 0x40,
            _ => return Some(form(b"CIA\0", CIA_FRAME, false)),
        };
        let record = tmd + 4 + sig_len + 0xC4 + 0x900;
        let kind = u16::from_be_bytes(p.get(record + 6..record + 8)?.try_into().ok()?);
        let decrypted = kind & 0x0001 == 0 && plain_ncch(p, content).unwrap_or(false);
        return Some(form(b"CIA\0", CIA_FRAME, decrypted));
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

    #[test]
    fn z3ds_forms_and_decrypted_files() {
        let decrypted = |mut h: Vec<u8>| {
            h[0x18F] |= 0x04;
            h
        };
        let cxi = ncch_header("CTR-P-AREE");
        assert_eq!(
            z3ds_form(&cxi),
            Some(Z3dsForm {
                magic: *b"NCCH",
                frame: FRAME,
                decrypted: false
            })
        );
        assert!(z3ds_form(&decrypted(cxi.clone())).unwrap().decrypted);
        let mut cci = vec![0u8; 0x4000];
        cci[0x100..0x104].copy_from_slice(b"NCSD");
        cci[0x120..0x124].copy_from_slice(&0x20u32.to_le_bytes());
        cci.extend(decrypted(cxi.clone()));
        assert_eq!(
            z3ds_form(&cci).map(|f| (f.magic, f.decrypted)),
            Some((*b"NCSD", true))
        );
        assert_eq!(z3ds_form(b"3DSX and more").map(|f| f.magic), Some(*b"3DSX"));
        // A CIA: the TMD (RSA-2048 signature) says whether content 0 is
        // encrypted; its NCCH must say NoCrypto too.
        let tmd_at = 0x2040 + 0xA00 + 0x380;
        let mut cia = vec![0u8; tmd_at + 0xB40];
        cia[0..4].copy_from_slice(&0x2020u32.to_le_bytes());
        cia[8..12].copy_from_slice(&0xA00u32.to_le_bytes());
        cia[12..16].copy_from_slice(&0x350u32.to_le_bytes());
        cia[16..20].copy_from_slice(&0xB34u32.to_le_bytes());
        cia[tmd_at..tmd_at + 4].copy_from_slice(&0x0001_0004u32.to_be_bytes());
        cia.extend(decrypted(cxi));
        let form = z3ds_form(&cia).unwrap();
        assert_eq!(
            (form.magic, form.frame, form.decrypted),
            (*b"CIA\0", CIA_FRAME, true)
        );
        cia[tmd_at + 0xB04 + 7] = 0x01;
        assert!(!z3ds_form(&cia).unwrap().decrypted, "content 0 marked encrypted");
    }
}
