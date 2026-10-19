use super::*;

/// A PPF 3 (no check block, no undo) of `records`.
pub(crate) fn ppf3(records: &[(u64, &[u8])]) -> Vec<u8> {
    let mut p = b"PPF30".to_vec();
    p.push(2);
    p.extend_from_slice(&[b' '; 50]);
    p.extend_from_slice(&[0, 0, 0, 0]); // BIN, no block, no undo, dummy
    for (at, data) in records {
        p.extend_from_slice(&at.to_le_bytes());
        p.push(data.len() as u8);
        p.extend_from_slice(data);
    }
    p
}

#[test]
fn ppf_versions_apply() {
    let source = vec![0u8; 64];
    let p3 = ppf3(&[(4, b"abc"), (60, b"wxyz")]);
    assert_eq!(info(&p3).map(|i| i.kind), Some(Kind::Ppf));
    let out = apply(&p3, &source).unwrap();
    assert_eq!(&out[4..7], b"abc");
    assert_eq!(out.len(), 64, "60 + 4 stays inside");
    // PPF 1: 32-bit offsets.
    let mut p1 = b"PPF10".to_vec();
    p1.push(0);
    p1.extend_from_slice(&[b' '; 50]);
    p1.extend_from_slice(&10u32.to_le_bytes());
    p1.push(2);
    p1.extend_from_slice(b"hi");
    let out = apply(&p1, &source).unwrap();
    assert_eq!(&out[10..12], b"hi");
    // With a FILE_ID.DIZ at the end (PPF 3).
    let mut with_diz = p3.clone();
    with_diz.extend_from_slice(b"@BEGIN_FILE_ID.DIZhello@END_FILE_ID.DIZ");
    with_diz.extend_from_slice(&5u32.to_le_bytes());
    assert_eq!(apply(&with_diz, &source).unwrap(), apply(&p3, &source).unwrap());
}

#[test]
fn ppf_check_block_refuses_another_image() {
    let mut image = vec![0u8; 0x9320 + 2048];
    image[0x9320..0x9320 + 1024].fill(7);
    let mut p = b"PPF30".to_vec();
    p.push(2);
    p.extend_from_slice(&[b' '; 50]);
    p.extend_from_slice(&[0, 1, 0, 0]); // BIN, a check block
    p.extend_from_slice(&[7u8; 1024]);
    p.extend_from_slice(&5u64.to_le_bytes());
    p.push(1);
    p.push(0xee);
    assert_eq!(apply(&p, &image).unwrap()[5], 0xee);
    image[0x9320] = 8;
    assert!(apply(&p, &image).is_none(), "another image");

    let dir = tempfile::tempdir().unwrap();
    let (src, out) = (dir.path().join("t.bin"), dir.path().join("o.bin"));
    std::fs::write(&src, &image).unwrap();
    let err = apply_file(&p, &src, &out).unwrap_err().to_string();
    assert!(err.contains("another image"), "{err}");
    assert!(!out.exists());
}

#[test]
fn xdelta_in_memory_and_streamed() {
    let source: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    let mut target = source.clone();
    target[1000..1010].copy_from_slice(b"TRANSLATED");
    target.extend_from_slice(b"more at the end");
    let mut delta = Vec::new();
    oxidelta::engine::encode(&source, &target, &mut delta).unwrap();
    assert_eq!(info(&delta).map(|i| i.kind), Some(Kind::Xdelta));
    assert_eq!(apply(&delta, &source).unwrap(), target);

    let dir = tempfile::tempdir().unwrap();
    let (src, out) = (dir.path().join("t.bin"), dir.path().join("o.bin"));
    std::fs::write(&src, &source).unwrap();
    apply_file(&delta, &src, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), target);
    let p = ppf3(&[(3, b"zz")]);
    let out2 = dir.path().join("p.bin");
    apply_file(&p, &src, &out2).unwrap();
    assert_eq!(&std::fs::read(&out2).unwrap()[3..5], b"zz");
}

fn num(mut n: u64, out: &mut Vec<u8>) {
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

fn footer(p: &mut Vec<u8>, source: &[u8], target: &[u8]) {
    p.extend_from_slice(&crc32fast::hash(source).to_le_bytes());
    p.extend_from_slice(&crc32fast::hash(target).to_le_bytes());
    let own = crc32fast::hash(p);
    p.extend_from_slice(&own.to_le_bytes());
}

/// A BPS that copies the source, then writes `tail` (TargetRead).
pub(crate) fn bps(source: &[u8], tail: &[u8]) -> Vec<u8> {
    let mut target = source.to_vec();
    target.extend_from_slice(tail);
    let mut p = b"BPS1".to_vec();
    num(source.len() as u64, &mut p);
    num(target.len() as u64, &mut p);
    num(0, &mut p);
    num((source.len() as u64 - 1) << 2, &mut p); // SourceRead
    num(((tail.len() as u64 - 1) << 2) | 1, &mut p);
    p.extend_from_slice(tail);
    footer(&mut p, source, &target);
    p
}

/// An IPS that writes `bytes` at `offset`.
pub(crate) fn ips(offset: u32, bytes: &[u8]) -> Vec<u8> {
    let mut p = b"PATCH".to_vec();
    p.extend_from_slice(&offset.to_be_bytes()[1..]);
    p.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    p.extend_from_slice(bytes);
    p.extend_from_slice(b"EOF");
    p
}

#[test]
fn bps_round_trip() {
    let source: Vec<u8> = (0..5000u32).map(|i| i as u8).collect();
    let p = bps(&source, b"translated!");
    let i = info(&p).unwrap();
    assert_eq!(i.kind, Kind::Bps);
    assert_eq!(i.source, Some((5000, crc32fast::hash(&source))));
    let out = apply(&p, &source).unwrap();
    assert_eq!(&out[5000..], b"translated!");
    assert_eq!(apply(&p, &source[1..]), None, "made from something else");
}

#[test]
fn ips_writes_rle_and_grows() {
    let source = vec![0u8; 16];
    let out = apply(&ips(4, b"HI"), &source).unwrap();
    assert_eq!(&out[4..6], b"HI");
    // RLE record past the end grows the file.
    let mut p = b"PATCH".to_vec();
    p.extend_from_slice(&[0, 0, 20, 0, 0, 0, 3, b'z']);
    p.extend_from_slice(b"EOF");
    let out = apply(&p, &source).unwrap();
    assert_eq!((out.len(), &out[20..23]), (23, &b"zzz"[..]));
    assert_eq!(info(&p).unwrap().kind, Kind::Ips);
    assert_eq!(info(b"not a patch"), None);
}

/// A forged BPS asking for a huge TargetCopy, or claiming absurd
/// metadata, fails at once instead of filling memory or panicking.
#[test]
fn hostile_bps_is_refused() {
    let source = vec![7u8; 16];
    let mut p = b"BPS1".to_vec();
    num(16, &mut p);
    num(32, &mut p);
    num(0, &mut p);
    num(1, &mut p); // TargetRead of 1 byte
    p.push(0xAA);
    num((((1u64 << 40) - 1) << 2) | 3, &mut p); // TargetCopy of 2^40 bytes
    num(0, &mut p);
    p.extend_from_slice(&[0u8; 12]);
    assert_eq!(apply_bps(&p, &source), None);
    let mut q = b"BPS1".to_vec();
    num(16, &mut q);
    num(16, &mut q);
    num(u64::MAX >> 8, &mut q);
    q.extend_from_slice(&[0u8; 12]);
    assert_eq!(apply_bps(&q, &source), None);
}

#[test]
fn ups_xor() {
    let source = b"HELLO WORLD".to_vec();
    let target = b"HELLO THERE".to_vec();
    let mut p = b"UPS1".to_vec();
    num(source.len() as u64, &mut p);
    num(target.len() as u64, &mut p);
    num(6, &mut p); // skip "HELLO "
    for (a, b) in source[6..].iter().zip(&target[6..]) {
        p.push(a ^ b);
    }
    p.push(0);
    footer(&mut p, &source, &target);
    assert_eq!(apply(&p, &source).unwrap(), target);
}
