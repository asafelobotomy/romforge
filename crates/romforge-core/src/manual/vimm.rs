//! Fetch one manual from Vimm's Lair when the user asks (never in bulk).
//!
//! Their PDF endpoint rejects romforge's User-Agent (HTTP 400). A browser
//! UA plus a Referer of the manual's page matches what their own form
//! sends. Sending a browser UA here is deliberate (the owner's decision,
//! to be kept): it is one download the user clicked for, as their own
//! browser would make it, not a crawl; everywhere else romforge says who
//! it is ([`fetch::USER_AGENT`]). When their FPDF builder fails, pages from `image.php` are
//! wrapped into a PDF instead.

use crate::Error;
use crate::fetch::{self, Request};

use super::{VIMM_PAGE, VIMM_PDF, is_pdf};

/// What their download form sends; without it, `dl.vimm.net` answers 400.
const BROWSER_UA: &str = concat!(
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 ",
    "(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
);

/// Fetch the PDF for Vimm manual `id` at about `dpi` (clamped to the
/// page's own max). Prefer their PDF form; if that isn't a PDF, build
/// one from their page JPEGs.
pub(super) fn fetch_pdf(id: &str, dpi: u32) -> Result<Vec<u8>, Error> {
    let page = format!("{VIMM_PAGE}{id}");
    let html = get_bytes(&page, &page)?.unwrap_or_default();
    let html = String::from_utf8_lossy(&html);
    let (max_dpi, pages) = page_meta(&html);
    let dpi = dpi.clamp(50, max_dpi.max(50));

    let pdf_url = format!("{VIMM_PDF}?manualId={id}&category=pdf&destDPI={dpi}");
    if let Ok(Some(bytes)) = get_bytes(&pdf_url, &page)
        && is_pdf(&bytes)
    {
        return Ok(bytes);
    }
    if pages == 0 {
        return Err(hand_download(&page));
    }
    assemble_from_images(id, &page, dpi, pages).map_err(|_| hand_download(&page))
}

fn hand_download(page: &str) -> Error {
    Error::Manual(format!(
        "Vimm's Lair wouldn't send the PDF; open {page}, download it there, and drop it on the Manual tab"
    ))
}

fn get_bytes(url: &str, referer: &str) -> Result<Option<Vec<u8>>, Error> {
    let req = Request {
        headers: vec![("User-Agent", BROWSER_UA.into()), ("Referer", referer.into())],
        max_body: Some(512 << 20),
        ..Request::get(url)
    };
    fetch::request(&req)
}

/// Max DPI from `ValidateForm(this, N)` / `max="N"`, and page count from
/// `Pages: </td><td>N`. Defaults: 100 dpi, unknown pages.
fn page_meta(html: &str) -> (u32, u32) {
    let max_dpi = after(html, "ValidateForm(this, ")
        .and_then(parse_u32)
        .or_else(|| html.split("max=\"").skip(1).filter_map(parse_u32).max())
        .unwrap_or(100);
    let pages = after(html, "Pages: </td><td>").and_then(parse_u32).unwrap_or(0);
    (max_dpi, pages)
}

fn after<'a>(hay: &'a str, needle: &str) -> Option<&'a str> {
    hay.split_once(needle).map(|(_, rest)| rest)
}

fn parse_u32(s: &str) -> Option<u32> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok().filter(|&n| n > 0)
}

fn assemble_from_images(id: &str, referer: &str, dpi: u32, pages: u32) -> Result<Vec<u8>, Error> {
    let mut jpegs = Vec::with_capacity(pages as usize);
    for p in 0..pages {
        let url = format!("{VIMM_PDF}image.php?type=manual&id={id}&page={p}&destDPI={dpi}");
        let Some(bytes) = get_bytes(&url, referer)? else {
            return Err(Error::Manual("Vimm's Lair page image missing".into()));
        };
        let Some(dims) = jpeg_size(&bytes) else {
            return Err(Error::Manual("Vimm's Lair page isn't a JPEG".into()));
        };
        // Their "Error: Image not found." banner is a tiny PNG; reject
        // anything too small to be a manual scan.
        if dims.0 < 100 || dims.1 < 100 || bytes.len() < 2_000 {
            return Err(Error::Manual("Vimm's Lair page image missing".into()));
        }
        jpegs.push((dims.0, dims.1, bytes));
    }
    jpegs_to_pdf(&jpegs)
}

/// One JPEG page → one PDF page (DCTDecode; no re-encode).
fn jpegs_to_pdf(pages: &[(u32, u32, Vec<u8>)]) -> Result<Vec<u8>, Error> {
    if pages.is_empty() {
        return Err(Error::Manual("no pages to put in a PDF".into()));
    }
    let n = pages.len();
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offs = Vec::with_capacity(2 + n * 3);

    let write_obj = |out: &mut Vec<u8>, offs: &mut Vec<usize>, num: usize, body: &str| {
        offs.push(out.len());
        debug_assert_eq!(offs.len(), num);
        out.extend_from_slice(format!("{num} 0 obj\n{body}\nendobj\n").as_bytes());
    };

    let mut kids = String::new();
    for i in 0..n {
        let page_n = 3 + i * 3 + 2;
        if i > 0 {
            kids.push(' ');
        }
        kids.push_str(&format!("{page_n} 0 R"));
    }
    write_obj(&mut out, &mut offs, 1, "<< /Type /Catalog /Pages 2 0 R >>");
    write_obj(
        &mut out,
        &mut offs,
        2,
        &format!("<< /Type /Pages /Kids [{kids}] /Count {n} >>"),
    );

    for (i, (w, h, jpeg)) in pages.iter().enumerate() {
        let img_n = 3 + i * 3;
        let content_n = img_n + 1;
        let page_n = img_n + 2;

        offs.push(out.len());
        debug_assert_eq!(offs.len(), img_n);
        out.extend_from_slice(
            format!(
                "{img_n} 0 obj\n<< /Type /XObject /Subtype /Image /Width {w} /Height {h} \
                 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode \
                 /Length {} >>\nstream\n",
                jpeg.len()
            )
            .as_bytes(),
        );
        out.extend_from_slice(jpeg);
        out.extend_from_slice(b"\nendstream\nendobj\n");

        let content = format!("q {w} 0 0 {h} 0 0 cm /Im0 Do Q\n");
        write_obj(
            &mut out,
            &mut offs,
            content_n,
            &format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        );
        write_obj(
            &mut out,
            &mut offs,
            page_n,
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] \
                 /Contents {content_n} 0 R \
                 /Resources << /XObject << /Im0 {img_n} 0 R >> >> >>"
            ),
        );
    }

    let xref_at = out.len();
    let count = offs.len() + 1; // + free object 0
    out.extend_from_slice(format!("xref\n0 {count}\n").as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for o in &offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n").as_bytes(),
    );
    Ok(out)
}

fn jpeg_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 4 || data[0] != 0xff || data[1] != 0xd8 {
        return None;
    }
    let mut i = 2;
    while i + 9 < data.len() {
        if data[i] != 0xff {
            return None;
        }
        let marker = data[i + 1];
        if marker == 0xd8 || marker == 0xd9 {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        // SOF0..SOF3 (baseline/progressive)
        if matches!(marker, 0xc0..=0xc3) && len >= 7 {
            let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_meta_reads_validate_form_and_pages() {
        let html = r#"
            <form onSubmit="return ValidateForm(this, 640)">
            <tr><td>Pages: </td><td>17</td></tr>
            <input name="destDPI" max="640" value="100">
        "#;
        assert_eq!(page_meta(html), (640, 17));
        assert_eq!(page_meta("<html></html>"), (100, 0));
        assert_eq!(
            page_meta(r#"ValidateForm(this, 100); Pages: </td><td>13</td>"#),
            (100, 13)
        );
    }

    #[test]
    fn jpeg_size_reads_sof() {
        // Minimal JPEG with a SOF0 for 1×1.
        let jpeg: &[u8] = &[
            0xff, 0xd8, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xff,
            0xd9,
        ];
        assert_eq!(jpeg_size(jpeg), Some((1, 1)));
        assert_eq!(jpeg_size(b"not a jpeg"), None);
        assert_eq!(jpeg_size(&[0xff, 0xd8, 0xff, 0xd9]), None);
    }

    #[test]
    #[ignore = "hits dl.vimm.net"]
    fn live_chrome_ua_gets_pdf() {
        let pdf = fetch_pdf("4770", 100).expect("Vimm PDF");
        assert!(is_pdf(&pdf));
        assert!(pdf.len() > 100_000);
    }

    #[test]
    fn jpegs_to_pdf_starts_with_pdf_header() {
        let mut img = image::RgbImage::new(8, 8);
        for p in img.pixels_mut() {
            *p = image::Rgb([200, 100, 50]);
        }
        let mut jpeg = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
            .unwrap();
        let (w, h) = jpeg_size(&jpeg).unwrap();
        let pdf = jpegs_to_pdf(&[(w, h, jpeg)]).unwrap();
        assert!(is_pdf(&pdf));
        assert!(pdf.windows(4).any(|w| w == b"/Im0"));
    }
}
