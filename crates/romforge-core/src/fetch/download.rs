//! Downloads too big to hold in memory, streamed to a file.

use super::request::{one_at_a_time, respond};
use super::*;

/// GET `url` straight into the file `to` (written as `<to>.part`, renamed
/// when complete), for downloads too big to hold in memory, `max` bytes at
/// most. `progress` gets (bytes so far, total if the server said). Returns
/// the size.
pub fn download_to(
    url: &str,
    to: &std::path::Path,
    max: u64,
    progress: impl FnMut(u64, Option<u64>),
) -> Result<u64, Error> {
    download_with(&Request::get(url), to, max, progress)
}

/// [`download_to`] for `req` (its headers, its deadline; `req.max_body` is
/// not used: `max` is).
pub fn download_with(
    req: &Request,
    to: &std::path::Path,
    max: u64,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<u64, Error> {
    let url = req.url;
    let err = |m: String| Error::Http(format!("{}: {m}", short(url)));
    let _one = one_at_a_time(url);
    let mut response = respond(req)?.ok_or_else(|| err("HTTP 404".into()))?;
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok()?.parse().ok());
    if let Some(t) = total.filter(|t| *t > max) {
        return Err(err(format!("{t} bytes, more than the {max} allowed")));
    }
    let part = to.with_extension(format!(
        "{}.part",
        to.extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let out = std::io::BufWriter::new(std::fs::File::create(&part)?);
    let body = response.body_mut().as_reader();
    let done = match write_body(body, out, max, total, &mut progress, &err) {
        Ok(done) => done,
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
    };
    if total.is_some_and(|t| t != done) {
        let _ = std::fs::remove_file(&part);
        return Err(err(format!(
            "download cut short ({done} of {} bytes)",
            total.unwrap_or(0)
        )));
    }
    std::fs::rename(&part, to)?;
    Ok(done)
}

/// [`download_to`]'s copy from `body` into `out`; the caller removes the
/// part file when this fails.
fn write_body(
    mut body: impl Read,
    mut out: impl std::io::Write,
    max: u64,
    total: Option<u64>,
    progress: &mut impl FnMut(u64, Option<u64>),
    err: &impl Fn(String) -> Error,
) -> Result<u64, Error> {
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        let n = match body.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => return Err(err(e.to_string())),
        };
        done += n as u64;
        if done > max {
            return Err(err(format!("more than the {max} bytes allowed")));
        }
        out.write_all(&buf[..n])?;
        progress(done, total);
        if !super::watch::report(done, total) {
            return Err(Error::Cancelled);
        }
    }
    out.flush()?;
    Ok(done)
}
