//! Writing Z3DS as Azahar 2126.2 does (`common/file_derived.cpp`,
//! `zstd_compression.cpp`): the header, the metadata (a version byte,
//! then items of type, name length, data length, name and data, an end
//! item, padded to 16 bytes), then zstd's seekable format at the default
//! level: independent frames of the file, then a skippable frame with each
//! frame's compressed and plain size (zstd's `contrib/seekable_format`).

use std::io::{self, Read, Seek, SeekFrom, Write};

use super::{HEADER_LEN, MAGIC, VERSION};

/// zstd's default level, as Azahar uses (`ZSTD_CLEVEL_DEFAULT`).
const LEVEL: i32 = 3;
const SKIPPABLE_SEEK_TABLE: u32 = 0x184D_2A5E;
const SEEKABLE_MAGIC: u32 = 0x8F92_EAB1;

/// The metadata items, as Azahar lays them out.
fn metadata(frame: usize) -> Vec<u8> {
    let date = crate::db::now_utc();
    let items = [
        ("compressor", format!("romforge {}", env!("CARGO_PKG_VERSION"))),
        ("date", date),
        ("maxframesize", frame.to_string()),
    ];
    let mut out = vec![1u8];
    for (name, data) in items {
        out.push(1);
        out.push(name.len() as u8);
        out.extend_from_slice(&(data.len() as u16).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data.as_bytes());
    }
    out.extend_from_slice(&[0; 4]);
    out.resize(out.len().div_ceil(16) * 16, 0);
    out
}

/// Compress `src` (exactly `size` bytes, a file whose own magic is
/// `magic`) into `out` as Z3DS, in frames of `frame` bytes on `threads`
/// threads.
pub fn write(
    mut src: impl Read,
    size: u64,
    magic: [u8; 4],
    frame: usize,
    threads: usize,
    out: &mut (impl Write + Seek),
) -> io::Result<()> {
    let start = out.stream_position()?;
    let meta = metadata(frame);
    out.write_all(&[0; HEADER_LEN])?;
    out.write_all(&meta)?;
    let mut frames: Vec<(u32, u32)> = Vec::new();
    let mut compressed = 0u64;
    let mut plain = 0u64;
    crate::chunks::in_order(
        &mut src,
        frame,
        threads,
        |_, c| crate::chunks::zstd(c, LEVEL),
        |data, n| {
            out.write_all(&data)?;
            compressed += data.len() as u64;
            plain += n as u64;
            frames.push((data.len() as u32, n as u32));
            Ok(())
        },
    )?;
    if plain != size {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("Z3DS: read {plain} bytes of {size}"),
        ));
    }
    let mut table = Vec::with_capacity(frames.len() * 8 + 17);
    table.extend_from_slice(&SKIPPABLE_SEEK_TABLE.to_le_bytes());
    table.extend_from_slice(&((frames.len() * 8 + 9) as u32).to_le_bytes());
    for (c, d) in &frames {
        table.extend_from_slice(&c.to_le_bytes());
        table.extend_from_slice(&d.to_le_bytes());
    }
    table.extend_from_slice(&(frames.len() as u32).to_le_bytes());
    table.push(0);
    table.extend_from_slice(&SEEKABLE_MAGIC.to_le_bytes());
    out.write_all(&table)?;
    compressed += table.len() as u64;
    let mut head = Vec::with_capacity(HEADER_LEN);
    head.extend_from_slice(&MAGIC);
    head.extend_from_slice(&magic);
    head.extend_from_slice(&[VERSION, 0]);
    head.extend_from_slice(&(HEADER_LEN as u16).to_le_bytes());
    head.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    head.extend_from_slice(&compressed.to_le_bytes());
    head.extend_from_slice(&size.to_le_bytes());
    let end = out.stream_position()?;
    out.seek(SeekFrom::Start(start))?;
    out.write_all(&head)?;
    out.seek(SeekFrom::Start(end))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_z3ds_reads_back_as_the_file() {
        let data: Vec<u8> = (0..700_000u32)
            .map(|i| (i % 13) as u8 ^ (i >> 11) as u8)
            .collect();
        let mut out = std::io::Cursor::new(Vec::new());
        write(&data[..], data.len() as u64, *b"NCSD", 256 * 1024, 2, &mut out).unwrap();
        let file = out.into_inner();
        assert!(file.len() < data.len() / 4, "it compresses");
        let header = super::super::parse(&file).unwrap();
        assert_eq!(&header.underlying_magic, b"NCSD");
        assert_eq!(header.uncompressed_size, data.len() as u64);
        assert_eq!(header.metadata_size % 16, 0);
        // Metadata as Azahar writes it: version 1, then named items.
        let meta = &file[HEADER_LEN..HEADER_LEN + header.metadata_size as usize];
        assert_eq!(meta[0], 1);
        assert_eq!(&meta[5..15], b"compressor");
        // The seek table closes the file: three frames.
        let tail = &file[file.len() - 9..];
        assert_eq!(u32::from_le_bytes(tail[..4].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(tail[5..].try_into().unwrap()), SEEKABLE_MAGIC);
        let (_, mut r) = super::super::open(std::io::Cursor::new(&file)).unwrap().unwrap();
        let mut back = Vec::new();
        r.read_to_end(&mut back).unwrap();
        assert_eq!(back, data);
        // Read on past the last frame: the seek table is skipped, then the end.
        let start = HEADER_LEN + header.metadata_size as usize;
        let mut all = Vec::new();
        super::super::Frames::new(&file[start..])
            .read_to_end(&mut all)
            .unwrap();
        assert_eq!(all, data);
        // A source shorter than said is an error.
        let mut out = std::io::Cursor::new(Vec::new());
        assert!(write(&data[..10], 20, *b"NCCH", 256 * 1024, 1, &mut out).is_err());
    }
}
