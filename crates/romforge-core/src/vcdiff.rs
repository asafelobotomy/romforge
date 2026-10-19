//! xdelta patches (VCDIFF, RFC 3284) as xdelta3 writes them: what
//! `oxidelta` doesn't read itself.
//!
//! - DJW and FGK secondary compression (xdelta3's `-S djw`, `-S fgk`): a
//!   patch using them is rewritten as plain VCDIFF (each compressed section
//!   decompressed, the window lengths fixed), which `oxidelta` decodes.
//!   LZMA, xdelta3's default, `oxidelta` reads as it is.
//! - The application header xdelta3 writes: the target's and source's file
//!   names, which say which track of a disc a patch was made from.
//!
//! The DJW and FGK decoders are ported from xdelta3 (`xdelta3-djw.h`,
//! `xdelta3-fgk.h`, `xdelta3-second.h`), Copyright 2016 Joshua MacDonald,
//! licensed under the Apache License, Version 2.0
//! (<http://www.apache.org/licenses/LICENSE-2.0>); this file is a modified
//! version of those parts (translated to Rust, decoding only) under the
//! same licence.

use std::borrow::Cow;

mod bits;
mod djw;
mod fgk;
#[cfg(test)]
mod tests;

use djw::djw_decode;
use fgk::Fgk;

const MAGIC: [u8; 4] = [0xd6, 0xc3, 0xc4, 0x00];
const VCD_SECONDARY: u8 = 0x01;
const VCD_CODETABLE: u8 = 0x02;
const VCD_APPHEADER: u8 = 0x04;
const VCD_SOURCE: u8 = 0x01;
const VCD_TARGET: u8 = 0x02;
const VCD_ADLER32: u8 = 0x04;
const VCD_DATACOMP: u8 = 0x01;
const VCD_INSTCOMP: u8 = 0x02;
const VCD_ADDRCOMP: u8 = 0x04;
const DJW_ID: u8 = 1;
const FGK_ID: u8 = 16;

type Res<T> = Result<T, String>;

/// A VCDIFF integer: base 128, most significant first, the high bit set on
/// all but the last byte.
fn read_int(d: &[u8], at: &mut usize) -> Res<u64> {
    let mut v: u64 = 0;
    for _ in 0..10 {
        let b = *d.get(*at).ok_or("a truncated patch")?;
        *at += 1;
        v = v.checked_mul(128).ok_or("a damaged patch")? | u64::from(b & 0x7f);
        if b & 0x80 == 0 {
            return Ok(v);
        }
    }
    Err("a damaged patch".into())
}

fn write_int(mut v: u64, out: &mut Vec<u8>) {
    let mut tmp = [0u8; 10];
    let mut n = 0;
    loop {
        tmp[n] = (v & 0x7f) as u8;
        n += 1;
        v >>= 7;
        if v == 0 {
            break;
        }
    }
    for i in (0..n).rev() {
        out.push(tmp[i] | if i > 0 { 0x80 } else { 0 });
    }
}

fn take<'a>(d: &'a [u8], at: &mut usize, n: u64) -> Res<&'a [u8]> {
    let n = usize::try_from(n).map_err(|_| "a damaged patch")?;
    let s = d
        .get(*at..at.checked_add(n).ok_or("a damaged patch")?)
        .ok_or("a truncated patch")?;
    *at += n;
    Ok(s)
}

type Header<'a> = (u8, Option<u8>, usize, Option<&'a [u8]>);

/// The file header: (indicator, secondary compressor, where the windows
/// start, the application header).
fn header(d: &[u8]) -> Res<Header<'_>> {
    if d.get(..4) != Some(&MAGIC) {
        return Err("not an xdelta (VCDIFF) patch".into());
    }
    let ind = *d.get(4).ok_or("a truncated patch")?;
    let mut at = 5;
    let mut secondary = None;
    if ind & VCD_SECONDARY != 0 {
        secondary = Some(*d.get(at).ok_or("a truncated patch")?);
        at += 1;
    }
    if ind & VCD_CODETABLE != 0 {
        let n = read_int(d, &mut at)?;
        take(d, &mut at, n)?;
    }
    let mut app = None;
    if ind & VCD_APPHEADER != 0 {
        let n = read_int(d, &mut at)?;
        app = Some(take(d, &mut at, n)?);
    }
    Ok((ind, secondary, at, app))
}

/// The file names xdelta3 recorded: (target, source), without folders.
pub(crate) fn names(patch: &[u8]) -> Option<(String, String)> {
    let (_, _, _, app) = header(patch).ok()?;
    let text = String::from_utf8_lossy(app?).into_owned();
    // "target/compression/source/compression"
    let parts: Vec<&str> = text.split('/').collect();
    let base = |s: &str| s.rsplit(['/', '\\']).next().unwrap_or(s).to_owned();
    match parts.as_slice() {
        [t, _, s, ..] if !s.is_empty() => Some((base(t), base(s))),
        _ => None,
    }
}

/// The patch as plain VCDIFF: as it is unless it uses DJW or FGK.
pub(crate) fn plain(patch: &[u8]) -> Res<Cow<'_, [u8]>> {
    let (ind, secondary, start, _) = header(patch)?;
    let Some(id) = secondary.filter(|id| *id == DJW_ID || *id == FGK_ID) else {
        return Ok(Cow::Borrowed(patch));
    };
    let mut out = Vec::with_capacity(patch.len() * 4);
    out.extend_from_slice(&MAGIC);
    out.push(ind & !VCD_SECONDARY);
    // Code table and application header as they were.
    out.extend_from_slice(&patch[6..start]);
    // FGK adapts as it goes, one coder per kind of section.
    let mut fgk = [Fgk::new(), Fgk::new(), Fgk::new()];
    let mut at = start;
    while at < patch.len() {
        let win = patch[at];
        at += 1;
        out.push(win);
        if win & (VCD_SOURCE | VCD_TARGET) != 0 {
            let size = read_int(patch, &mut at)?;
            let pos = read_int(patch, &mut at)?;
            write_int(size, &mut out);
            write_int(pos, &mut out);
        }
        let enc_len = read_int(patch, &mut at)?;
        let enc_start = at;
        let target_len = read_int(patch, &mut at)?;
        let del = *patch.get(at).ok_or("a truncated patch")?;
        at += 1;
        let lens = [
            read_int(patch, &mut at)?,
            read_int(patch, &mut at)?,
            read_int(patch, &mut at)?,
        ];
        let adler = if win & VCD_ADLER32 != 0 {
            Some(take(patch, &mut at, 4)?)
        } else {
            None
        };
        let mut sections: Vec<Cow<[u8]>> = Vec::new();
        for (i, (len, flag)) in lens
            .iter()
            .zip([VCD_DATACOMP, VCD_INSTCOMP, VCD_ADDRCOMP])
            .enumerate()
        {
            let s = take(patch, &mut at, *len)?;
            if del & flag == 0 {
                sections.push(Cow::Borrowed(s));
                continue;
            }
            let mut p = 0;
            let size = usize::try_from(read_int(s, &mut p)?).map_err(|_| "a damaged patch")?;
            if size > 256 << 20 {
                return Err("a section too big to be real".into());
            }
            let body = &s[p..];
            let dec = if id == DJW_ID {
                djw_decode(body, size)?
            } else {
                fgk[i].decode(body, size)?
            };
            sections.push(Cow::Owned(dec));
        }
        if at - enc_start != enc_len as usize {
            return Err("a damaged patch (window lengths disagree)".into());
        }
        let mut body = Vec::new();
        write_int(target_len, &mut body);
        body.push(0);
        for s in &sections {
            write_int(s.len() as u64, &mut body);
        }
        if let Some(a) = adler {
            body.extend_from_slice(a);
        }
        for s in &sections {
            body.extend_from_slice(s);
        }
        write_int(body.len() as u64, &mut out);
        out.extend_from_slice(&body);
    }
    Ok(Cow::Owned(out))
}
