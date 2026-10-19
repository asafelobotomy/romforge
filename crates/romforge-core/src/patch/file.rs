use std::path::Path;

use super::{Kind, apply, info, ppf_records};

/// Apply a patch to the file `source`, writing `out` (which must not
/// exist), without holding a disc image in memory: a PPF is copied then
/// changed in place, an xdelta decoded window by window reading the source
/// where it points. The others (cartridge formats) go through [`apply`].
/// On failure nothing is left at `out`.
pub fn apply_file(patch: &[u8], source: &Path, out: &Path) -> Result<(), crate::Error> {
    let mut made = false;
    let r = write(patch, source, out, &mut made);
    if r.is_err() && made {
        let _ = std::fs::remove_file(out);
    }
    r
}

/// [`apply_file`]; `made` says whether `out` was created.
fn write(patch: &[u8], source: &Path, out: &Path, made: &mut bool) -> Result<(), crate::Error> {
    use std::io::{Seek, SeekFrom, Write};
    let bad = |m: &str| crate::Error::Dat(format!("patch: {m}"));
    let create = |made: &mut bool| -> std::io::Result<std::fs::File> {
        let f = std::fs::File::create_new(out)?;
        *made = true;
        Ok(f)
    };
    let i = info(patch).ok_or_else(|| bad("not a patch romforge reads"))?;
    match i.kind {
        Kind::Ppf => {
            let ppf = ppf_records(patch).ok_or_else(|| bad("a damaged PPF"))?;
            let mut f = std::fs::File::open(source)?;
            if let Some((at, block)) = ppf.check {
                let mut have = vec![0u8; block.len()];
                f.seek(SeekFrom::Start(at))?;
                if std::io::Read::read_exact(&mut f, &mut have).is_err() || have != block {
                    return Err(bad(
                        "this PPF was made for another image (its check block differs)",
                    ));
                }
            }
            if ppf
                .size
                .is_some_and(|s| s != f.metadata().map(|m| m.len()).unwrap_or(0))
            {
                return Err(bad("this PPF was made for an image of another size"));
            }
            drop(f);
            let mut w = create(made)?;
            std::io::copy(&mut std::fs::File::open(source)?, &mut w)?;
            for (at, data) in &ppf.records {
                w.seek(SeekFrom::Start(*at))?;
                w.write_all(data)?;
            }
            w.sync_all()?;
            Ok(())
        }
        Kind::Xdelta => {
            let mut src = FileSource(std::fs::File::open(source)?, std::fs::metadata(source)?.len());
            let plain = crate::vcdiff::plain(patch).map_err(|e| bad(&format!("xdelta: {e}")))?;
            let mut w = std::io::BufWriter::new(create(made)?);
            oxidelta::compress::DeltaDecoder::new(&plain[..])
                .decode_to(&mut src, &mut w)
                .map_err(|e| bad(&format!("xdelta: {e}")))?;
            w.into_inner().map_err(|e| e.into_error())?.sync_all()?;
            Ok(())
        }
        _ => {
            let data = std::fs::read(source)?;
            let patched = apply(patch, &data).ok_or_else(|| bad("it doesn't fit this file"))?;
            let mut w = create(made)?;
            w.write_all(&patched)?;
            w.sync_all()?;
            Ok(())
        }
    }
}

/// An xdelta's source, read from a file where the patch points.
struct FileSource(std::fs::File, u64);

impl oxidelta::vcdiff::decoder::SourceProvider for FileSource {
    fn read_source(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, oxidelta::vcdiff::DecodeError> {
        use std::io::{Read, Seek, SeekFrom};
        let io = oxidelta::vcdiff::DecodeError::Io;
        self.0.seek(SeekFrom::Start(offset)).map_err(io)?;
        let mut n = 0;
        while n < buf.len() {
            match self.0.read(&mut buf[n..]).map_err(io)? {
                0 => break,
                k => n += k,
            }
        }
        Ok(n)
    }

    fn source_len(&self) -> Option<u64> {
        Some(self.1)
    }
}
