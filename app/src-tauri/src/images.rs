//! Images as `data:` URLs.

use std::path::Path;

/// Pictures over this are left out of Details: every one is sent to the
/// page as base64 (a third larger), and a fanart can be tens of MB.
const MAX_IMAGE: u64 = 8 << 20;

/// The image at `path` as a `data:` URL, if it is there and not too big.
pub fn image_url(path: &Path) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > MAX_IMAGE {
        return None;
    }
    Some(data_url(&std::fs::read(path).ok()?))
}

/// An image as a `data:` URL with its real type (PNG, JPEG or WebP).
fn data_url(img: &[u8]) -> String {
    let mime = match romforge_core::enrich::image_ext(img) {
        Some("jpg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    format!("data:{mime};base64,{}", base64(img))
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foo"), "Zm9v");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
    }
}
