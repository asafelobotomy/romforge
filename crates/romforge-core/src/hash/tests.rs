use super::*;

/// Reader that returns one byte per call, to exercise chunk boundaries.
struct Trickle<'a>(&'a [u8]);
impl Read for Trickle<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.0.is_empty() || out.is_empty() {
            return Ok(0);
        }
        out[0] = self.0[0];
        self.0 = &self.0[1..];
        Ok(1)
    }
}

#[test]
fn known_vectors() {
    let h = hash_stream(&b"abc"[..], 3).unwrap();
    assert_eq!(h.raw.crc32, "352441c2");
    assert_eq!(h.raw.md5, "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(h.raw.sha1, "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert!(h.stripped.is_none());
}

#[test]
fn stripped_equals_hash_of_body() {
    let body: Vec<u8> = (0..1024u32).map(|i| (i * 7) as u8).collect();
    let mut file = b"NES\x1a".to_vec();
    file.resize(16, 0);
    file.extend_from_slice(&body);

    let headered = hash_stream(Trickle(&file), file.len() as u64).unwrap();
    let plain = hash_stream(&body[..], body.len() as u64).unwrap();
    let (kind, stripped) = headered.stripped.unwrap();
    assert_eq!(kind, "ines");
    assert_eq!(stripped, plain.raw);
    assert_ne!(headered.raw, plain.raw);
}

/// `.v64`, `.n64` and `.smd` dumps hash as the DAT's form, also when
/// read a byte at a time and in big chunks.
#[test]
fn other_byte_orders_hash_as_the_dats() {
    let mut z64: Vec<u8> = (0..64 * 1024u32).map(|i| (i * 13 + i / 251) as u8).collect();
    z64[..4].copy_from_slice(&[0x80, 0x37, 0x12, 0x40]);
    let plain = hash_stream(&z64[..], z64.len() as u64).unwrap();
    assert!(plain.stripped.is_none(), "z64 is the DAT's order");
    let v64: Vec<u8> = z64.chunks(2).flat_map(|w| [w[1], w[0]]).collect();
    let n64: Vec<u8> = z64.chunks(4).flat_map(|w| [w[3], w[2], w[1], w[0]]).collect();
    // Super Magic Drive: header, then odd bytes and even bytes per 16 KiB.
    let mut smd = vec![0u8; 512];
    smd[1] = 3;
    smd[8..10].copy_from_slice(&[0xAA, 0xBB]);
    for block in z64.chunks(16 * 1024) {
        smd.extend(block.iter().skip(1).step_by(2));
        smd.extend(block.iter().step_by(2));
    }
    for (file, want) in [(&v64, "n64-v64"), (&n64, "n64-n64"), (&smd, "smd")] {
        for h in [
            hash_stream(Trickle(file), file.len() as u64).unwrap(),
            hash_stream(&file[..], file.len() as u64).unwrap(),
        ] {
            let (kind, d) = h.stripped.unwrap();
            assert_eq!(kind, want);
            assert_eq!(d, plain.raw, "{want}");
        }
    }
}

/// A big stream (MD5 and SHA1 on their own threads) hashes exactly as
/// the same data in one thread.
#[test]
fn big_streams_hash_the_same_split() {
    let data: Vec<u8> = (0..(SPLIT_AT + 12345))
        .map(|i| ((i * 2654435761) >> 13) as u8)
        .collect();
    let split = hash_stream(&data[..], data.len() as u64).unwrap().raw;
    let mut one = Hasher::default();
    one.update(&data);
    assert_eq!(split, one.finish());
    // A short read is still an error, and nothing hangs.
    assert!(hash_stream(&data[..SPLIT_AT as usize], data.len() as u64).is_err());
}

#[test]
fn size_mismatch_is_an_error() {
    assert!(hash_stream(&b"abc"[..], 4).is_err());
}
