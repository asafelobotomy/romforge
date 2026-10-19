//! Thumbnails of art for the library's picture views: a box scan, a 3D
//! render or a fanart scaled down to the height a card draws it at, so a
//! grid of thousands decodes kilobytes, not megabytes, per card.
//!
//! Kept under the cache dir (`thumbs/`), named by the source's path, size
//! and modification time and the height, so a changed picture gets a new
//! thumbnail and an unchanged one is made once. JPEG when the picture is
//! opaque, PNG when it has see-through parts (3D renders, logos).

use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

use image::{DynamicImage, ImageFormat};
use sha2::{Digest as _, Sha256};

use crate::Error;

const JPEG_QUALITY: u8 = 85;
/// Heights outside this are served as they are (nothing to gain, or a
/// request that isn't a card's).
const MIN_HEIGHT: u32 = 16;
const MAX_HEIGHT: u32 = 2048;

/// Where thumbnails live under a cache dir.
pub fn dir(cache: &Path) -> PathBuf {
    cache.join("thumbs")
}

/// The picture to show for `src` drawn `height` px tall: its thumbnail,
/// made (once) under `cache` if there isn't one for this version of the
/// file yet, or `src` itself when it is no taller than that already.
pub fn thumbnail(src: &Path, height: u32, cache: &Path) -> Result<PathBuf, Error> {
    if !(MIN_HEIGHT..=MAX_HEIGHT).contains(&height) {
        return Ok(src.to_path_buf());
    }
    let meta = std::fs::metadata(src)?;
    let mtime = meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut h = Sha256::new();
    h.update(src.as_os_str().as_encoded_bytes());
    h.update([0]);
    h.update(meta.len().to_le_bytes());
    h.update(mtime.to_le_bytes());
    h.update(height.to_le_bytes());
    let key: String = h.finalize()[..16].iter().map(|b| format!("{b:02x}")).collect();
    let dir = dir(cache);
    let base = dir.join(&key);
    for ext in ["jpg", "png", "src"] {
        let made = base.with_extension(ext);
        if made.is_file() {
            // `.src`: the picture is small already; remembered so it isn't
            // decoded again just to find that out.
            return Ok(if ext == "src" { src.to_path_buf() } else { made });
        }
    }

    let img = image::ImageReader::open(src)?
        .with_guessed_format()?
        .decode()
        .map_err(|e| Error::Image(format!("{}: {e}", src.display())))?;
    std::fs::create_dir_all(&dir)?;
    if img.height() <= height {
        write_new(&base.with_extension("src"), |_| Ok(()))?;
        return Ok(src.to_path_buf());
    }
    let small = img.thumbnail(u32::MAX, height);
    let (out, format) = if opaque(&small) {
        (base.with_extension("jpg"), ImageFormat::Jpeg)
    } else {
        (base.with_extension("png"), ImageFormat::Png)
    };
    write_new(&out, |f| {
        let mut w = BufWriter::new(f);
        match format {
            ImageFormat::Jpeg => {
                let rgb = DynamicImage::ImageRgb8(small.to_rgb8());
                rgb.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
                    &mut w,
                    JPEG_QUALITY,
                ))
            }
            _ => small.write_to(&mut w, format),
        }
        .map_err(|e| Error::Image(e.to_string()))?;
        w.into_inner().map_err(|e| e.into_error())?;
        Ok(())
    })?;
    Ok(out)
}

/// No pixel lets anything through.
fn opaque(img: &DynamicImage) -> bool {
    !img.color().has_alpha() || img.to_rgba8().pixels().all(|p| p.0[3] == 255)
}

/// Write `path` through a temporary file beside it, so a request made at
/// the same time never sees half a picture.
fn write_new(path: &Path, fill: impl FnOnce(&std::fs::File) -> Result<(), Error>) -> Result<(), Error> {
    static N: AtomicU64 = AtomicU64::new(0);
    let tmp = path.with_extension(format!(
        "tmp{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let result = std::fs::File::create(&tmp)
        .map_err(Error::from)
        .and_then(|f| fill(&f))
        .and_then(|()| std::fs::rename(&tmp, path).map_err(Error::from));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
