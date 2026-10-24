//! Milestone 25: Wii U game IDs, from plain fields only (nothing is
//! decrypted). Read in Cemu 2.6's source:
//! - a WUD (`Filesystem/FST/FST.cpp`): the product code at 0
//!   (`WUP-P-ARPE…`), magic `CC 54 9E B9` at 0x10000; no maker code;
//! - a WUX (`Filesystem/WUD/wud.cpp`, little-endian): `WUX0`,
//!   `0x1099D02E`, the sector size, the WUD's size at 0x10; a u32 per
//!   sector from 0x20, then the sectors, aligned to their size;
//! - a WUA (`crate::wua`) and a decrypted title folder: the base title's
//!   `meta/meta.xml` (`product_code`, `company_code`, `longname_en`).
//!
//! The serial is the game code with its maker when known (`ARPE01`, as
//! GameCube and Wii IDs are), else the game code alone.

use std::io::{Read, Seek};
use std::path::Path;

use super::{GameId, code_region, id};

const WUD_MAGIC: [u8; 4] = [0xCC, 0x54, 0x9E, 0xB9];
const WUX_MAGIC: u32 = 0x1099_D02E;

/// A WUD's game code, from its first bytes.
fn product_code(p: &[u8]) -> Option<String> {
    let code = p.strip_prefix(b"WUP-P-")?.get(..4)?;
    code.iter()
        .all(u8::is_ascii_alphanumeric)
        .then(|| String::from_utf8_lossy(code).into_owned())
}

fn game(code: String, maker: Option<String>, title: Option<String>) -> GameId {
    let region = code_region(code.as_bytes()[3]);
    id("wiiu", Some(code + maker.as_deref().unwrap_or("")), title, region)
}

/// A WUD: the product code at 0 and the magic at 0x10000 (within the
/// prefix the readers get).
pub(super) fn wud(p: &[u8]) -> Option<GameId> {
    (p.get(0x10000..0x10004)? == WUD_MAGIC).then_some(())?;
    Some(game(product_code(p)?, None, None))
}

/// A WUX: sector 0 of the WUD it holds, found through its index.
pub(super) fn wux<R: Read + Seek>(f: &mut R, head: &[u8]) -> Option<GameId> {
    let le32 = |b: &[u8]| u32::from_le_bytes(b.try_into().unwrap_or_default());
    if head.get(..4)? != b"WUX0" || le32(head.get(4..8)?) != WUX_MAGIC {
        return None;
    }
    let sector = u64::from(le32(head.get(8..12)?));
    let size = u64::from_le_bytes(head.get(0x10..0x18)?.try_into().ok()?);
    // As Cemu checks it.
    if !(0x100..0x1000_0000).contains(&sector) || size == 0 {
        return None;
    }
    let entries = size.div_ceil(sector);
    let array = (0x20 + entries.checked_mul(4)?).div_ceil(sector) * sector;
    let mut first = [0u8; 4];
    super::containers::read_exact_at(f, 0x20, &mut first)?;
    let mut sector0 = vec![0u8; 0x20];
    let at = array.checked_add(u64::from(le32(&first)).checked_mul(sector)?)?;
    super::containers::read_exact_at(f, at, &mut sector0)?;
    Some(game(product_code(&sector0)?, None, None))
}

/// A `meta.xml`'s product code, maker and English title.
pub fn meta(xml: &str) -> Option<GameId> {
    let field = |tag: &str| -> Option<String> {
        let open = xml.find(&format!("<{tag}"))?;
        let from = open + xml[open..].find('>')? + 1;
        let to = from + xml[from..].find(&format!("</{tag}>"))?;
        let v = xml[from..to].trim();
        (!v.is_empty()).then(|| v.to_owned())
    };
    let code = product_code(field("product_code")?.as_bytes())?;
    // `company_code` is four digits ("0001"); GameTDB's maker is the last two.
    let maker = field("company_code")
        .filter(|c| c.len() == 4 && c.bytes().all(|b| b.is_ascii_alphanumeric()))
        .map(|c| c[2..].to_owned());
    let title = field("longname_en").map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "));
    Some(game(code, maker, title))
}

/// A WUA: its first base title's `meta.xml` (as Cemu picks: a game or a
/// demo, not an update or DLC).
pub(super) fn wua<R: Read + Seek>(f: &mut R) -> Option<GameId> {
    let mut archive = crate::wua::Archive::open(f)?;
    let base = archive.top_folders().into_iter().find(|n| {
        n.len() > 18
            && n.as_bytes()[16] == b'_'
            && matches!(&n[..8].to_ascii_lowercase()[..], "00050000" | "00050002")
    })?;
    let xml = archive.read(&format!("{base}/meta/meta.xml"))?;
    meta(&String::from_utf8_lossy(&xml))
}

/// A decrypted title folder's game file (`code/<x>.rpx` beside `content/`
/// and `meta/`) or an eShop download's `title.tmd`: what Cemu starts, and
/// what organize and cleanup leave in its folder.
pub fn in_title_folder(path: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase());
    let Some(name) = name else { return false };
    let Some(dir) = path.parent() else { return false };
    if name == "title.tmd" {
        return true;
    }
    name.ends_with(".rpx")
        && dir.file_name().is_some_and(|d| d.eq_ignore_ascii_case("code"))
        && dir
            .parent()
            .is_some_and(|t| t.join("content").is_dir() && t.join("meta").is_dir())
}

/// The ID a file's folder gives it: a decrypted title folder's `.rpx`
/// from its `meta/meta.xml`. Only reads.
pub fn beside(path: &Path) -> Option<GameId> {
    if !in_title_folder(path) || path.file_name()?.eq_ignore_ascii_case("title.tmd") {
        return None;
    }
    let meta_xml = path.parent()?.parent()?.join("meta/meta.xml");
    let mut text = String::new();
    std::fs::File::open(meta_xml)
        .ok()?
        .take(1 << 20)
        .read_to_string(&mut text)
        .ok()?;
    meta(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const META: &str = "<?xml version=\"1.0\"?>\n<menu type=\"complex\">\n  <product_code type=\"string\" length=\"32\">WUP-P-ARPE</product_code>\n  <company_code type=\"string\" length=\"8\">0001</company_code>\n  <longname_en type=\"string\" length=\"512\">Mario Kart 8\n</longname_en>\n</menu>";

    #[test]
    fn discs_archives_and_folders_give_the_product_code() {
        // WUD: the code at 0, the magic at 0x10000; no maker.
        let mut wud = vec![0u8; 0x10004];
        wud[..16].copy_from_slice(b"WUP-P-ARPE-0-00\0");
        wud[0x10000..].copy_from_slice(&WUD_MAGIC);
        let id = super::super::read(&wud).unwrap();
        assert_eq!(
            (id.kind, id.serial.as_deref(), id.region.as_deref()),
            ("wiiu", Some("ARPE"), Some("USA"))
        );
        assert!(super::super::read(&wud[..0x10000]).is_none(), "no magic, no ID");

        // WUX: sector 0 reached through the index (here stored second).
        let sector = 0x100u32;
        let size = 3 * u64::from(sector);
        let mut wux = b"WUX0".to_vec();
        wux.extend_from_slice(&WUX_MAGIC.to_le_bytes());
        wux.extend_from_slice(&sector.to_le_bytes());
        wux.extend_from_slice(&[0; 4]);
        wux.extend_from_slice(&size.to_le_bytes());
        wux.extend_from_slice(&[0; 8]);
        for i in [1u32, 0, 0] {
            wux.extend_from_slice(&i.to_le_bytes());
        }
        wux.resize(0x100, 0);
        wux.extend(std::iter::repeat_n(0u8, 0x100));
        let mut s0 = b"WUP-P-AMKP".to_vec();
        s0.resize(0x100, 0);
        wux.extend_from_slice(&s0);
        let id = super::super::read_container(&mut std::io::Cursor::new(&wux)).unwrap();
        assert_eq!(
            (id.serial.as_deref(), id.region.as_deref()),
            (Some("AMKP"), Some("Europe"))
        );

        // WUA: the base title's meta.xml, not the update's.
        let file = crate::wua::tests::wua_of(&[
            (
                "0005000e10101e00_v64/meta/meta.xml",
                b"<product_code>WUP-P-XXXX</product_code>",
            ),
            ("0005000010101e00_v0/meta/meta.xml", META.as_bytes()),
        ]);
        let id = super::super::read_container(&mut std::io::Cursor::new(&file)).unwrap();
        assert_eq!(
            (id.serial.as_deref(), id.title.as_deref()),
            (Some("ARPE01"), Some("Mario Kart 8"))
        );

        // A decrypted folder: the .rpx gets its folder's meta.xml.
        let t = tempfile::tempdir().unwrap();
        for d in ["code", "content", "meta"] {
            std::fs::create_dir_all(t.path().join("Game").join(d)).unwrap();
        }
        std::fs::write(t.path().join("Game/meta/meta.xml"), META).unwrap();
        let rpx = t.path().join("Game/code/Turbo.rpx");
        std::fs::write(&rpx, b"\x7fELF").unwrap();
        assert!(in_title_folder(&rpx) && in_title_folder(Path::new("/x/title.tmd")));
        assert_eq!(beside(&rpx).unwrap().serial.as_deref(), Some("ARPE01"));
        assert!(!in_title_folder(&t.path().join("Game/Turbo.rpx")));
    }
}
