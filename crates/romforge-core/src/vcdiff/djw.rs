// Ported from xdelta3 (`xdelta3-djw.h`, `xdelta3-fgk.h`,
// `xdelta3-second.h`), Copyright 2016 Joshua MacDonald, licensed under the
// Apache License, Version 2.0 (<http://www.apache.org/licenses/LICENSE-2.0>);
// this file is a modified version of those parts (translated to Rust,
// decoding only) under the same licence.

use super::Res;
use super::bits::Bits;

// ------------------------------------------------------------------------
// DJW: static Huffman with move-to-front coded code lengths
// (xdelta3-djw.h).

pub(super) const ALPHABET: usize = 256;
const DJW_MAX_CODELEN: usize = 20;
const DJW_TOTAL_CODES: usize = DJW_MAX_CODELEN + 2;
const RUN_1: usize = 1;
const DJW_EXTRA_12OFFSET: usize = 7;
const DJW_EXTRA_CODE_BITS: u32 = 4;
const DJW_MAX_GROUPS: usize = 8;
const DJW_GROUP_BITS: u32 = 3;
const DJW_SECTORSZ_MULT: usize = 5;
const DJW_SECTORSZ_BITS: u32 = 5;
const DJW_MAX_CLCLEN: usize = 15;
const DJW_CLCLEN_BITS: u32 = 4;
const DJW_MAX_GBCLEN: usize = 7;
const DJW_GBCLEN_BITS: u32 = 3;
const DJW_12BASIC: [u8; 5] = [4, 5, 6, 7, 8];
const DJW_12EXTRA: [u8; 15] = [9, 10, 3, 11, 2, 12, 13, 1, 14, 15, 16, 17, 18, 19, 20];

/// A canonical Huffman decoder.
struct Table {
    inorder: Vec<usize>,
    base: [i64; DJW_TOTAL_CODES + 1],
    limit: [i64; DJW_TOTAL_CODES + 1],
    min: usize,
    max: usize,
}

fn build_decoder(clen: &[u8], abs_max: usize) -> Res<Table> {
    let mut nr = [0i64; DJW_TOTAL_CODES + 1];
    for &c in clen {
        let c = c as usize;
        if c > abs_max {
            return Err("secondary decoder invalid code length".into());
        }
        nr[c] += 1;
    }
    let mut min = 1;
    while min <= abs_max && nr[min] == 0 {
        min += 1;
    }
    let mut max = abs_max;
    while max != 0 && nr[max] == 0 {
        max -= 1;
    }
    let mut t = Table {
        inorder: vec![0; clen.len()],
        base: [0; DJW_TOTAL_CODES + 1],
        limit: [0; DJW_TOTAL_CODES + 1],
        min,
        max,
    };
    if min > abs_max {
        return Ok(t); // no codes at all
    }
    let mut tmp_base = [0i64; DJW_TOTAL_CODES + 1];
    t.limit[min] = nr[min] - 1;
    for i in min + 1..=max {
        let last_limit = (t.limit[i - 1] + 1) << 1;
        tmp_base[i] = tmp_base[i - 1] + nr[i - 1];
        t.limit[i] = last_limit + nr[i] - 1;
        t.base[i] = last_limit - tmp_base[i];
    }
    for (i, &l) in clen.iter().enumerate() {
        if l != 0 {
            let at = tmp_base[l as usize] as usize;
            t.inorder[at] = i;
            tmp_base[l as usize] += 1;
        }
    }
    Ok(t)
}

fn decode_symbol(bits: &mut Bits, t: &Table, max_sym: usize) -> Res<usize> {
    let bad = || "secondary decoder invalid code".to_owned();
    let mut code: i64 = 0;
    let mut n = 0usize;
    loop {
        if bits.cur_mask == 0x100 {
            bits.next_byte()?;
        }
        while bits.cur_mask != 0x100 {
            if n == t.max {
                return Err(bad());
            }
            n += 1;
            code <<= 1;
            if bits.cur_byte & bits.cur_mask != 0 {
                code |= 1;
            }
            bits.cur_mask <<= 1;
            if n >= t.min && code <= t.limit[n] {
                if t.base[n] <= code {
                    let offset = (code - t.base[n]) as usize;
                    if offset <= max_sym {
                        return t.inorder.get(offset).copied().ok_or_else(bad);
                    }
                }
                return Err(bad());
            }
        }
    }
}

/// Move entry `i` of the list to its front, returning it.
fn update_mtf(mtf: &mut [u8], i: usize) -> u8 {
    let sym = mtf[i];
    mtf.copy_within(0..i, 1);
    mtf[0] = sym;
    sym
}

/// Values coded as move-to-front positions with RUN_0 / RUN_1 repeats.
fn decode_1_2(
    bits: &mut Bits,
    t: &Table,
    mtfvals: &mut [u8],
    elts: usize,
    skip_offset: usize,
    values: &mut [u8],
) -> Res<()> {
    let (mut n, mut rep, mut mtf, mut s) = (0usize, 0usize, 0usize, 0u32);
    while n < elts {
        if skip_offset != 0 && n >= skip_offset && values[n - skip_offset] == 0 {
            values[n] = 0;
            n += 1;
            continue;
        }
        if rep != 0 {
            values[n] = mtfvals[0];
            n += 1;
            rep -= 1;
            continue;
        }
        if mtf != 0 {
            if mtf >= mtfvals.len() {
                return Err("secondary decoder invalid code".into());
            }
            values[n] = update_mtf(mtfvals, mtf);
            n += 1;
            mtf = 0;
            continue;
        }
        mtf = decode_symbol(bits, t, DJW_TOTAL_CODES)?;
        if mtf <= RUN_1 {
            rep = (mtf + 1)
                .checked_shl(s)
                .ok_or("secondary decoder invalid repeat code")?;
            mtf = 0;
            s += 1;
        } else {
            mtf -= 1;
            s = 0;
        }
    }
    if rep != 0 {
        return Err("secondary decoder invalid repeat code".into());
    }
    Ok(())
}

pub(super) fn djw_decode(input: &[u8], out_len: usize) -> Res<Vec<u8>> {
    if out_len == 0 {
        return Err("secondary decoder invalid input".into());
    }
    let mut bits = Bits::new(input);
    let groups = bits.read(DJW_GROUP_BITS)? + 1;
    let sector_size = if groups > 1 {
        (bits.read(DJW_SECTORSZ_BITS)? + 1) * DJW_SECTORSZ_MULT
    } else {
        out_len
    };
    let sectors = 1 + (out_len - 1) / sector_size;

    // The code-length code, then each group's code lengths.
    let extra = bits.read(DJW_EXTRA_CODE_BITS)? + DJW_EXTRA_12OFFSET;
    let mut cl_clen = [0u8; DJW_TOTAL_CODES];
    for c in cl_clen.iter_mut().take(extra) {
        *c = bits.read(DJW_CLCLEN_BITS)? as u8;
    }
    let cl = build_decoder(&cl_clen, DJW_MAX_CLCLEN)?;
    let mut cl_mtf = [0u8; DJW_TOTAL_CODES];
    cl_mtf[1..6].copy_from_slice(&DJW_12BASIC);
    cl_mtf[6..21].copy_from_slice(&DJW_12EXTRA);
    let mut clen = vec![0u8; ALPHABET * groups];
    decode_1_2(
        &mut bits,
        &cl,
        &mut cl_mtf,
        ALPHABET * groups,
        ALPHABET,
        &mut clen,
    )?;
    let tables: Vec<Table> = (0..groups)
        .map(|g| build_decoder(&clen[g * ALPHABET..(g + 1) * ALPHABET], DJW_MAX_CODELEN))
        .collect::<Res<_>>()?;

    // Which group codes each sector.
    let mut sel_group = vec![0u8; sectors];
    if groups > 1 {
        let mut sel_clen = [0u8; DJW_MAX_GROUPS + 1];
        let mut sel_mtf = [0u8; DJW_MAX_GROUPS + 2];
        for g in 0..=groups {
            sel_clen[g] = bits.read(DJW_GBCLEN_BITS)? as u8;
            sel_mtf[g] = g as u8;
        }
        let sel = build_decoder(&sel_clen[..=groups], DJW_MAX_GBCLEN)?;
        decode_1_2(
            &mut bits,
            &sel,
            &mut sel_mtf[..=groups],
            sectors,
            0,
            &mut sel_group,
        )?;
    }

    let mut out = Vec::with_capacity(out_len);
    for &g in &sel_group {
        let t = tables.get(g as usize).ok_or("secondary decoder invalid group")?;
        let n = sector_size.min(out_len - out.len());
        for _ in 0..n {
            let sym = decode_symbol(&mut bits, t, ALPHABET)?;
            out.push(u8::try_from(sym).map_err(|_| "secondary decoder invalid code")?);
        }
    }
    Ok(out)
}
