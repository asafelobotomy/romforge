use super::*;

#[test]
fn gba_header() {
    let mut rom = vec![0u8; 0xC0];
    rom[0xA0..0xAC].copy_from_slice(b"POKEMON EMER");
    rom[0xAC..0xB0].copy_from_slice(b"BPEE");
    rom[0xB0..0xB2].copy_from_slice(b"01");
    rom[0xB2] = 0x96;
    let sum = rom[0xA0..0xBD].iter().fold(0u8, |a, &b| a.wrapping_sub(b));
    rom[0xBD] = sum.wrapping_sub(0x19);
    let id = read(&rom).unwrap();
    assert_eq!(id.kind, "gba");
    assert_eq!(id.serial.as_deref(), Some("BPEE"));
    assert_eq!(id.title.as_deref(), Some("POKEMON EMER"));
    assert_eq!(id.region.as_deref(), Some("USA"));
    rom[0xBD] ^= 1;
    assert_eq!(read(&rom), None, "a bad checksum is no header");
}

#[test]
fn snes_lorom_with_game_code() {
    let mut rom = vec![0xFFu8; 0x8000];
    let h = 0x7FC0;
    rom[h..h + 21].copy_from_slice(b"DRAGONQUEST3         ");
    rom[h + 0x19] = 0;
    rom[h + 0x1A] = 0x33;
    rom[h - 0x0E..h - 0x0A].copy_from_slice(b"AQ3J");
    rom[h + 0x1C..h + 0x20].copy_from_slice(&[0x1A, 0x93, 0xE5, 0x6C]);
    let id = read(&rom).unwrap();
    assert_eq!(id.kind, "snes");
    assert_eq!(id.title.as_deref(), Some("DRAGONQUEST3"));
    assert_eq!(id.serial.as_deref(), Some("AQ3J"));
    assert_eq!(id.region.as_deref(), Some("Japan"));
}

#[test]
fn mega_drive_serial_without_its_type() {
    let mut rom = vec![0u8; 0x200];
    rom[0x100..0x110].copy_from_slice(b"SEGA GENESIS    ");
    rom[0x150..0x166].copy_from_slice(b"CASTLEVANIA BLOODLINES");
    rom[0x180..0x18E].copy_from_slice(b"GM T-95076-00 ");
    rom[0x1F0] = b'U';
    let id = read(&rom).unwrap();
    assert_eq!(id.kind, "md");
    assert_eq!(id.serial.as_deref(), Some("T-95076-00"));
    assert_eq!(id.title.as_deref(), Some("CASTLEVANIA BLOODLINES"));
    assert_eq!(id.region.as_deref(), Some("U"));
}

#[test]
fn n64_and_game_boy() {
    let mut z64 = vec![0u8; 0x40];
    z64[..4].copy_from_slice(&[0x80, 0x37, 0x12, 0x40]);
    z64[0x20..0x2E].copy_from_slice(b"SUPER MARIO 64");
    z64[0x3B..0x3F].copy_from_slice(b"NSME");
    let id = read(&z64).unwrap();
    assert_eq!((id.kind, id.serial.as_deref()), ("n64", Some("NSME")));
    assert_eq!(id.region.as_deref(), Some("USA"));

    let mut gb = vec![0u8; 0x150];
    gb[0x134..0x13A].copy_from_slice(b"TETRIS");
    gb[0x14A] = 1;
    gb[0x14D] = gb[0x134..0x14D]
        .iter()
        .fold(0u8, |a, &b| a.wrapping_sub(b).wrapping_sub(1));
    let id = read(&gb).unwrap();
    assert_eq!((id.kind, id.title.as_deref()), ("gb", Some("TETRIS")));
}

/// A tiny ISO 9660 image with SYSTEM.CNF in its root, as raw Mode 2
/// sectors like a PS1 BIN track.
fn ps1_disc(cnf: &str) -> Vec<u8> {
    let mut user = vec![[0u8; 2048]; 20];
    let pvd = &mut user[16];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    let root = &mut pvd[156..190];
    root[0] = 34;
    root[2..6].copy_from_slice(&18u32.to_le_bytes());
    root[10..14].copy_from_slice(&2048u32.to_le_bytes());
    let dir = &mut user[18];
    let name = b"SYSTEM.CNF;1";
    dir[0] = (33 + name.len()) as u8;
    dir[2..6].copy_from_slice(&19u32.to_le_bytes());
    dir[10..14].copy_from_slice(&(cnf.len() as u32).to_le_bytes());
    dir[32] = name.len() as u8;
    dir[33..33 + name.len()].copy_from_slice(name);
    user[19][..cnf.len()].copy_from_slice(cnf.as_bytes());
    let mut raw = Vec::new();
    for s in &user {
        let mut sector = [0u8; 2352];
        sector[..12].copy_from_slice(&SYNC);
        sector[15] = 2;
        sector[24..24 + 2048].copy_from_slice(s);
        raw.extend_from_slice(&sector);
    }
    raw
}

#[test]
fn ps1_and_ps2_boot_files() {
    let id = read(&ps1_disc("BOOT = cdrom:\\SLUS_007.07;1\r\nTCB = 4\r\n")).unwrap();
    assert_eq!((id.kind, id.serial.as_deref()), ("ps1", Some("SLUS_007.07")));
    let id = read(&ps1_disc("BOOT2 = cdrom0:\\SLUS_203.12;1\nVER = 1.00\n")).unwrap();
    assert_eq!((id.kind, id.serial.as_deref()), ("ps2", Some("SLUS_203.12")));
    assert_eq!(read(&ps1_disc("nothing here")), None);
}

#[test]
fn boot_file_named_in_the_root_when_system_cnf_is_far() {
    // SYSTEM.CNF's data beyond the prefix: the root's SLUS_007.47 tells.
    let mut disc = ps1_disc("BOOT = cdrom:\\SLUS_007.47;1\r\n");
    let dir = 18 * 2352 + 24;
    let at = dir + disc[dir] as usize;
    let name = b"SLUS_007.47;1";
    disc[at] = (33 + name.len()) as u8;
    disc[at + 32] = name.len() as u8;
    disc[at + 33..at + 33 + name.len()].copy_from_slice(name);
    // Point SYSTEM.CNF far away.
    disc[dir + 2..dir + 6].copy_from_slice(&293_729u32.to_le_bytes());
    let id = read(&disc).unwrap();
    assert_eq!((id.kind, id.serial.as_deref()), ("ps1", Some("SLUS_007.47")));
    assert!(boot_name("SCES_123.45") && !boot_name("ZNULL.DAT") && !boot_name("SLUS-00747"));
}

#[test]
fn saturn_and_gamecube() {
    let mut ip = vec![0u8; 2048];
    ip[..16].copy_from_slice(b"SEGA SEGASATURN ");
    ip[0x20..0x2A].copy_from_slice(b"MK-81009  ");
    ip[0x40..0x41].copy_from_slice(b"U");
    ip[0x60..0x6F].copy_from_slice(b"NIGHTS INTO DRE");
    let id = read(&ip).unwrap();
    assert_eq!((id.kind, id.serial.as_deref()), ("saturn", Some("MK-81009")));

    let mut gc = vec![0u8; 0x60];
    gc[..6].copy_from_slice(b"GALE01");
    gc[0x1C..0x20].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]);
    gc[0x20..0x34].copy_from_slice(b"Super Smash Bros. Me");
    let id = read(&gc).unwrap();
    assert_eq!((id.kind, id.serial.as_deref()), ("gc", Some("GALE01")));
    assert_eq!(id.region.as_deref(), Some("USA"));
}

/// A GameCube disc's first bytes.
fn gc_disc() -> Vec<u8> {
    let mut d = vec![0u8; 0x8000];
    d[..6].copy_from_slice(b"GALE01");
    d[0x1C..0x20].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]);
    d[0x20..0x31].copy_from_slice(b"Super Smash Bros.");
    d
}

fn container(bytes: &[u8]) -> Option<GameId> {
    read_container(&mut std::io::Cursor::new(bytes))
}

#[test]
fn gamecube_and_wii_containers() {
    let disc = gc_disc();
    // RVZ / WIA: big-endian ISO size at 0x24, disc head at 0x58.
    let mut rvz = vec![0u8; 0x100];
    rvz[..4].copy_from_slice(b"RVZ\x01");
    rvz[0x24..0x2C].copy_from_slice(&1_459_978_240u64.to_be_bytes());
    rvz[0x58..0xD8].copy_from_slice(&disc[..0x80]);
    let id = container(&rvz).unwrap();
    assert_eq!((id.kind, id.serial.as_deref()), ("gc", Some("GALE01")));
    assert_eq!(id.image_size, Some(1_459_978_240));
    // WBFS: disc head after the first HD sector (2^9 = 512).
    let mut wbfs = vec![0u8; 0x400];
    wbfs[..4].copy_from_slice(b"WBFS");
    wbfs[8] = 9;
    wbfs[0x200..0x300].copy_from_slice(&disc[..0x100]);
    assert_eq!(container(&wbfs).unwrap().serial.as_deref(), Some("GALE01"));
    // Wii/GC CISO: block size, map, disc at 0x8000.
    let mut ciso = vec![0u8; 0x8000];
    ciso[..4].copy_from_slice(b"CISO");
    ciso[4..8].copy_from_slice(&0x20_0000u32.to_le_bytes());
    ciso.extend_from_slice(&disc[..0x100]);
    assert_eq!(container(&ciso).unwrap().serial.as_deref(), Some("GALE01"));
    // GCZ: header, 2 block pointers and hashes, zlib blocks.
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    std::io::Write::write_all(&mut z, &disc).unwrap();
    let block0 = z.finish().unwrap();
    let mut gcz = Vec::new();
    gcz.extend_from_slice(&0xB10B_C001u32.to_le_bytes());
    gcz.extend_from_slice(&0u32.to_le_bytes());
    gcz.extend_from_slice(&0u64.to_le_bytes());
    gcz.extend_from_slice(&1_459_978_240u64.to_le_bytes());
    gcz.extend_from_slice(&0x8000u32.to_le_bytes());
    gcz.extend_from_slice(&2u32.to_le_bytes());
    gcz.extend_from_slice(&0u64.to_le_bytes());
    gcz.extend_from_slice(&(block0.len() as u64).to_le_bytes());
    gcz.extend_from_slice(&[0u8; 8]);
    gcz.extend_from_slice(&block0);
    let id = container(&gcz).unwrap();
    assert_eq!(
        (id.serial.as_deref(), id.image_size),
        (Some("GALE01"), Some(1_459_978_240))
    );
}

/// A plain 2048-byte-sector ISO with one file in its root.
fn iso(name: &str, content: &[u8]) -> Vec<u8> {
    let mut user = vec![[0u8; 2048]; 24];
    let pvd = &mut user[16];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[156] = 34;
    pvd[158..162].copy_from_slice(&18u32.to_le_bytes());
    pvd[166..170].copy_from_slice(&2048u32.to_le_bytes());
    let n = format!("{name};1");
    let dir = &mut user[18];
    dir[0] = (33 + n.len()) as u8;
    dir[2..6].copy_from_slice(&19u32.to_le_bytes());
    dir[10..14].copy_from_slice(&(content.len() as u32).to_le_bytes());
    dir[32] = n.len() as u8;
    dir[33..33 + n.len()].copy_from_slice(n.as_bytes());
    user[19][..content.len()].copy_from_slice(content);
    user.concat()
}

/// CSO (deflate) or ZSO (LZ4) of `image`, 2048-byte blocks.
fn cso(image: &[u8], lz4: bool) -> Vec<u8> {
    let blocks: Vec<Vec<u8>> = image
        .chunks(2048)
        .map(|b| {
            if lz4 {
                lz4_flex::block::compress(b)
            } else {
                let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
                std::io::Write::write_all(&mut e, b).unwrap();
                e.finish().unwrap()
            }
        })
        .collect();
    let mut out = Vec::new();
    out.extend_from_slice(if lz4 { b"ZISO" } else { b"CISO" });
    out.extend_from_slice(&0x18u32.to_le_bytes());
    out.extend_from_slice(&(image.len() as u64).to_le_bytes());
    out.extend_from_slice(&2048u32.to_le_bytes());
    out.extend_from_slice(&[1, 0, 0, 0]);
    let mut at = 0x18 + 4 * (blocks.len() as u32 + 1);
    let mut index = Vec::new();
    for b in &blocks {
        index.extend_from_slice(&at.to_le_bytes());
        at += b.len() as u32;
    }
    index.extend_from_slice(&at.to_le_bytes());
    out.extend_from_slice(&index);
    for b in &blocks {
        out.extend_from_slice(b);
    }
    out
}

#[test]
fn psp_cso_zso_and_pbp() {
    let image = iso("UMD_DATA.BIN", b"ULUS-10041|0001|G|");
    assert_eq!(read(&image).unwrap().serial.as_deref(), Some("ULUS-10041"));
    for lz4 in [false, true] {
        let id = container(&cso(&image, lz4)).unwrap();
        assert_eq!(
            (id.kind, id.serial.as_deref()),
            ("psp", Some("ULUS-10041")),
            "lz4 {lz4}"
        );
        assert_eq!(id.image_size, Some(image.len() as u64));
    }
    // PBP: PARAM.SFO with DISC_ID, TITLE and CATEGORY ME (a PS1 classic).
    let fields = [
        ("CATEGORY", "ME"),
        ("DISC_ID", "SLUS00067"),
        ("TITLE", "Castlevania SOTN"),
    ];
    let mut keys = Vec::new();
    let mut data = Vec::new();
    let mut entries = Vec::new();
    for (k, v) in fields {
        entries.extend_from_slice(&(keys.len() as u16).to_le_bytes());
        entries.extend_from_slice(&0x0204u16.to_le_bytes());
        entries.extend_from_slice(&(v.len() as u32 + 1).to_le_bytes());
        entries.extend_from_slice(&(v.len() as u32 + 1).to_le_bytes());
        entries.extend_from_slice(&(data.len() as u32).to_le_bytes());
        keys.extend_from_slice(k.as_bytes());
        keys.push(0);
        data.extend_from_slice(v.as_bytes());
        data.push(0);
    }
    let keys_at = 20 + entries.len();
    let mut sfo = b"\0PSF".to_vec();
    sfo.extend_from_slice(&0x0101u32.to_le_bytes());
    sfo.extend_from_slice(&(keys_at as u32).to_le_bytes());
    sfo.extend_from_slice(&((keys_at + keys.len()) as u32).to_le_bytes());
    sfo.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    sfo.extend_from_slice(&entries);
    sfo.extend_from_slice(&keys);
    sfo.extend_from_slice(&data);
    let mut pbp = b"\0PBP".to_vec();
    pbp.extend_from_slice(&0x10000u32.to_le_bytes());
    let after = 0x28 + sfo.len() as u32;
    pbp.extend_from_slice(&0x28u32.to_le_bytes());
    for _ in 0..7 {
        pbp.extend_from_slice(&after.to_le_bytes());
    }
    pbp.extend_from_slice(&sfo);
    let id = container(&pbp).unwrap();
    assert_eq!((id.kind, id.serial.as_deref()), ("ps1", Some("SLUS00067")));
    assert_eq!(id.title.as_deref(), Some("Castlevania SOTN"));
    assert_eq!(container(&[0x5A; 512]), None);
}

/// Hostile CSO headers (a shift past 31, an absurd stored block, a
/// huge block size) give no ID instead of panicking or allocating.
#[test]
fn hostile_cso_headers() {
    let good = cso(&iso("UMD_DATA.BIN", b"ULUS-10041|0001|G|"), false);
    assert!(container(&good).is_some());
    let mut shift = good.clone();
    shift[21] = 40;
    assert_eq!(container(&shift), None);
    let mut block = good.clone();
    block[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(container(&block), None);
    let mut stored = good.clone();
    stored[21] = 31;
    stored[0x1C..0x20].copy_from_slice(&0x7FFF_FFFFu32.to_le_bytes());
    assert_eq!(container(&stored), None);
}

#[test]
fn every_kind_has_a_system() {
    for k in KINDS {
        assert!(crate::systems::by_ident(k).next().is_some(), "{k} has no system");
    }
}

#[test]
fn plain_data_has_no_id() {
    assert_eq!(read(&[0x5Au8; 4096]), None);
    assert_eq!(read(&[]), None);
}
