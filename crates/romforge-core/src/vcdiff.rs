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

// ------------------------------------------------------------------------
// Bits (xdelta3-second.h): least significant bit of each byte first.

struct Bits<'a> {
    input: &'a [u8],
    pos: usize,
    cur_byte: u32,
    cur_mask: u32,
}

impl<'a> Bits<'a> {
    fn new(input: &'a [u8]) -> Self {
        Bits {
            input,
            pos: 0,
            cur_byte: 0,
            cur_mask: 0x100,
        }
    }

    fn next_byte(&mut self) -> Res<()> {
        self.cur_byte = u32::from(*self.input.get(self.pos).ok_or("secondary decoder end of input")?);
        self.pos += 1;
        self.cur_mask = 1;
        Ok(())
    }

    /// `nbits` bits, the first read the most significant.
    fn read(&mut self, nbits: u32) -> Res<usize> {
        let mut value = 0usize;
        let mut vmask = 1usize << nbits;
        loop {
            if self.cur_mask == 0x100 {
                self.next_byte()?;
            }
            while self.cur_mask != 0x100 {
                vmask >>= 1;
                if self.cur_byte & self.cur_mask != 0 {
                    value |= vmask;
                }
                self.cur_mask <<= 1;
                if vmask == 1 {
                    return Ok(value);
                }
            }
        }
    }
}

// ------------------------------------------------------------------------
// DJW: static Huffman with move-to-front coded code lengths
// (xdelta3-djw.h).

const ALPHABET: usize = 256;
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

fn djw_decode(input: &[u8], out_len: usize) -> Res<Vec<u8>> {
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

// ------------------------------------------------------------------------
// FGK: adaptive Huffman (Knuth, "Dynamic Huffman Coding"; xdelta3-fgk.h).
// The tree's nodes and blocks are indices into two arrays.

const NONE: usize = usize::MAX;

#[derive(Clone, Copy)]
struct Node {
    weight: u32,
    parent: usize,
    left_child: usize,
    right_child: usize,
    left: usize,
    right: usize,
    my_block: usize,
}

struct Fgk {
    nodes: Vec<Node>,
    /// A block's leader node, or (free) the next free block.
    blocks: Vec<usize>,
    free_block: usize,
    zero_freq_count: usize,
    zero_freq_exp: usize,
    zero_freq_rem: usize,
    coded_depth: usize,
    coded_bits: Vec<u8>,
    decode_ptr: usize,
    remaining_zeros: usize,
    root: usize,
    free_node: usize,
}

impl Fgk {
    fn new() -> Self {
        let total_nodes = 2 * ALPHABET - 1;
        let total_blocks = 2 * total_nodes;
        let mut h = Fgk {
            nodes: vec![
                Node {
                    weight: 0,
                    parent: NONE,
                    left_child: NONE,
                    right_child: NONE,
                    left: NONE,
                    right: NONE,
                    my_block: NONE,
                };
                total_nodes
            ],
            blocks: (1..=total_blocks)
                .map(|i| if i == total_blocks { NONE } else { i })
                .collect(),
            free_block: 0,
            zero_freq_count: ALPHABET + 2,
            zero_freq_exp: 0,
            zero_freq_rem: 0,
            coded_depth: 0,
            coded_bits: vec![0; ALPHABET],
            decode_ptr: 0,
            remaining_zeros: 0,
            root: 0,
            free_node: ALPHABET,
        };
        h.factor_remaining();
        h.factor_remaining();
        for i in 0..ALPHABET {
            let n = &mut h.nodes[i];
            n.right_child = if i < ALPHABET - 1 { i + 1 } else { NONE };
            n.left_child = if i >= 1 { i - 1 } else { NONE };
        }
        h
    }

    fn factor_remaining(&mut self) {
        self.zero_freq_count -= 1;
        let mut i = self.zero_freq_count;
        self.zero_freq_exp = 0;
        while i > 1 {
            self.zero_freq_exp += 1;
            i >>= 1;
        }
        self.zero_freq_rem = self.zero_freq_count - (1 << self.zero_freq_exp);
    }

    fn make_block(&mut self, lead: usize) -> Res<usize> {
        let b = self.free_block;
        if b == NONE {
            return Err("secondary decoder out of blocks".into());
        }
        self.free_block = self.blocks[b];
        self.blocks[b] = lead;
        Ok(b)
    }

    fn free_block(&mut self, b: usize) {
        self.blocks[b] = self.free_block;
        self.free_block = b;
    }

    fn update_tree(&mut self, n: usize) -> Res<()> {
        let mut incr = if self.nodes[n].weight == 0 {
            self.increase_zero_weight(n)?
        } else {
            n
        };
        while incr != self.root {
            self.move_right(incr);
            self.promote(incr)?;
            self.nodes[incr].weight += 1;
            incr = self.nodes[incr].parent;
            if incr == NONE {
                return Err("secondary decoder broken tree".into());
            }
        }
        self.nodes[self.root].weight += 1;
        Ok(())
    }

    fn move_right(&mut self, fwd: usize) {
        let back = self.blocks[self.nodes[fwd].my_block];
        if fwd == back || self.nodes[fwd].parent == back || self.nodes[fwd].weight == 0 {
            return;
        }
        let back_right = self.nodes[back].right;
        self.nodes[back_right].left = fwd;
        if self.nodes[fwd].left != NONE {
            let fl = self.nodes[fwd].left;
            self.nodes[fl].right = back;
        }
        let tmp = self.nodes[fwd].right;
        self.nodes[fwd].right = self.nodes[back].right;
        if tmp == back {
            self.nodes[back].right = fwd;
        } else {
            self.nodes[tmp].left = back;
            self.nodes[back].right = tmp;
        }
        let tmp = self.nodes[back].left;
        self.nodes[back].left = self.nodes[fwd].left;
        if tmp == fwd {
            self.nodes[fwd].left = back;
        } else {
            self.nodes[tmp].right = fwd;
            self.nodes[fwd].left = tmp;
        }
        // Swap the two nodes' places under their parents.
        let fp = self.nodes[fwd].parent;
        let bp = self.nodes[back].parent;
        let fwd_is_right = self.nodes[fp].right_child == fwd;
        let back_is_right = self.nodes[bp].right_child == back;
        self.nodes[fwd].parent = bp;
        self.nodes[back].parent = fp;
        // fwd_par_ptr <-> back_par_ptr: the parents' child slots.
        let set = |nodes: &mut Vec<Node>, parent: usize, right: bool, v: usize| {
            if right {
                nodes[parent].right_child = v;
            } else {
                nodes[parent].left_child = v;
            }
        };
        set(&mut self.nodes, fp, fwd_is_right, back);
        set(&mut self.nodes, bp, back_is_right, fwd);
        let blk = self.nodes[fwd].my_block;
        self.blocks[blk] = fwd;
    }

    fn promote(&mut self, node: usize) -> Res<()> {
        let my_right = self.nodes[node].right;
        let my_left = self.nodes[node].left;
        let cur_block = self.nodes[node].my_block;
        if self.nodes[node].weight == 0 {
            return Ok(());
        }
        let lc = self.nodes[node].left_child;
        if my_left == self.nodes[node].right_child && lc != NONE && self.nodes[lc].weight == 0 {
            if self.nodes[node].weight + 1 == self.nodes[my_right].weight && my_right != self.root {
                self.free_block(cur_block);
                let rb = self.nodes[my_right].my_block;
                self.nodes[node].my_block = rb;
                self.nodes[my_left].my_block = rb;
            }
            return Ok(());
        }
        if my_left == self.remaining_zeros {
            return Ok(());
        }
        if self.nodes[my_left].my_block == cur_block {
            self.blocks[cur_block] = my_left;
        } else {
            self.free_block(cur_block);
        }
        if self.nodes[node].weight + 1 == self.nodes[my_right].weight && my_right != self.root {
            self.nodes[node].my_block = self.nodes[my_right].my_block;
        } else {
            self.nodes[node].my_block = self.make_block(node)?;
        }
        Ok(())
    }

    fn increase_zero_weight(&mut self, n: usize) -> Res<usize> {
        let this_zero = n;
        if self.zero_freq_count == 1 {
            self.nodes[this_zero].right_child = NONE;
            let r = self.nodes[this_zero].right;
            if self.nodes[r].weight == 1 {
                self.nodes[this_zero].my_block = self.nodes[r].my_block;
            } else {
                self.nodes[this_zero].my_block = self.make_block(this_zero)?;
            }
            self.remaining_zeros = NONE;
            return Ok(this_zero);
        }
        let zero_ptr = self.remaining_zeros;
        let new_internal = self.free_node;
        if new_internal >= self.nodes.len() {
            return Err("secondary decoder out of nodes".into());
        }
        self.free_node += 1;
        self.nodes[new_internal].parent = self.nodes[zero_ptr].parent;
        self.nodes[new_internal].right = self.nodes[zero_ptr].right;
        self.nodes[new_internal].weight = 0;
        self.nodes[new_internal].right_child = this_zero;
        self.nodes[new_internal].left = this_zero;
        if self.remaining_zeros == self.root {
            self.root = new_internal;
            self.nodes[this_zero].my_block = self.make_block(this_zero)?;
            self.nodes[new_internal].my_block = self.make_block(new_internal)?;
        } else {
            let r = self.nodes[new_internal].right;
            self.nodes[r].left = new_internal;
            let zp = self.nodes[zero_ptr].parent;
            if self.nodes[zp].right_child == zero_ptr {
                self.nodes[zp].right_child = new_internal;
            } else {
                self.nodes[zp].left_child = new_internal;
            }
            if self.nodes[r].weight == 1 {
                self.nodes[new_internal].my_block = self.nodes[r].my_block;
            } else {
                self.nodes[new_internal].my_block = self.make_block(new_internal)?;
            }
            self.nodes[this_zero].my_block = self.nodes[new_internal].my_block;
        }
        self.eliminate_zero(this_zero);
        let rz = self.remaining_zeros;
        self.nodes[new_internal].left_child = rz;
        self.nodes[this_zero].right = new_internal;
        self.nodes[this_zero].left = rz;
        self.nodes[this_zero].parent = new_internal;
        self.nodes[this_zero].left_child = NONE;
        self.nodes[this_zero].right_child = NONE;
        self.nodes[rz].parent = new_internal;
        self.nodes[rz].right = this_zero;
        Ok(this_zero)
    }

    fn eliminate_zero(&mut self, node: usize) {
        if self.zero_freq_count == 1 {
            return;
        }
        self.factor_remaining();
        let (lc, rc) = (self.nodes[node].left_child, self.nodes[node].right_child);
        if lc == NONE {
            self.remaining_zeros = self.nodes[self.remaining_zeros].right_child;
            let rz = self.remaining_zeros;
            self.nodes[rz].left_child = NONE;
        } else if rc == NONE {
            self.nodes[lc].right_child = NONE;
        } else {
            self.nodes[rc].left_child = lc;
            self.nodes[lc].right_child = rc;
        }
    }

    /// One bit in; whether a symbol is complete.
    fn decode_bit(&mut self, b: bool) -> Res<bool> {
        if self.nodes[self.decode_ptr].weight == 0 {
            let need = self.zero_freq_exp + usize::from(self.zero_freq_rem != 0);
            *self
                .coded_bits
                .get_mut(self.coded_depth)
                .ok_or("secondary decoder invalid code")? = u8::from(b);
            self.coded_depth += 1;
            return Ok(self.coded_depth >= need);
        }
        let next = if b {
            self.nodes[self.decode_ptr].right_child
        } else {
            self.nodes[self.decode_ptr].left_child
        };
        if next == NONE {
            return Err("secondary decoder invalid code".into());
        }
        self.decode_ptr = next;
        if self.nodes[next].left_child == NONE {
            if self.nodes[next].weight != 0 {
                return Ok(true);
            }
            return Ok(self.zero_freq_count == 1);
        }
        Ok(false)
    }

    fn decode_data(&mut self) -> Res<u8> {
        let mut elt = self.decode_ptr;
        if self.nodes[self.decode_ptr].weight == 0 {
            let mut n = 0usize;
            let mut i = 0;
            if self.coded_depth > 0 {
                while i < self.coded_depth - 1 {
                    n |= self.coded_bits[i] as usize;
                    n <<= 1;
                    i += 1;
                }
            }
            n |= self.coded_bits[i] as usize;
            // The nth remaining zero-frequency symbol.
            let mut r = self.remaining_zeros;
            while n != 0 && self.nodes[r].right_child != NONE {
                r = self.nodes[r].right_child;
                n -= 1;
            }
            elt = r;
        }
        if elt >= ALPHABET {
            return Err("secondary decoder invalid code".into());
        }
        self.coded_depth = 0;
        self.update_tree(elt)?;
        self.decode_ptr = self.root;
        Ok(elt as u8)
    }

    fn decode(&mut self, input: &[u8], out_len: usize) -> Res<Vec<u8>> {
        let mut out = Vec::with_capacity(out_len);
        if out_len == 0 {
            return Ok(out);
        }
        for &byte in input {
            let mut mask = 1u32;
            while mask != 0x100 {
                if self.decode_bit(u32::from(byte) & mask != 0)? {
                    out.push(self.decode_data()?);
                    if out.len() == out_len {
                        return Ok(out);
                    }
                }
                mask <<= 1;
            }
        }
        Err("secondary decoder end of input".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_round_trip() {
        for v in [0u64, 1, 127, 128, 300, 16384, u32::MAX as u64] {
            let mut b = Vec::new();
            write_int(v, &mut b);
            let mut at = 0;
            assert_eq!(read_int(&b, &mut at).unwrap(), v);
            assert_eq!(at, b.len());
        }
    }

    /// The source and target `fixtures/patches/*.xdelta` were made from,
    /// by xdelta3 (`-S djw -W 16384`, and `-S fgk` with an xdelta3 built
    /// with `SECONDARY_FGK`).
    fn vectors() -> (Vec<u8>, Vec<u8>) {
        let mut x: u32 = 1;
        let src: Vec<u8> = (0..65536)
            .map(|_| {
                x = x.wrapping_mul(1103515245).wrapping_add(12345) & 0x7fff_ffff;
                (x >> 16) as u8
            })
            .collect();
        let text = b"the quick brown fox jumps over the lazy dog, said the translator. ";
        let tgt = (0..65536usize)
            .map(|i| {
                if (i / 4096) % 2 == 0 {
                    src[i]
                } else {
                    text[i % text.len()] ^ u8::from((i / text.len()).is_multiple_of(5))
                }
            })
            .collect();
        (src, tgt)
    }

    fn fixture(name: &str) -> Vec<u8> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/patches")
            .join(name);
        std::fs::read(p).unwrap()
    }

    #[test]
    fn djw_and_fgk_patches_decode() {
        let (src, tgt) = vectors();
        for name in ["djw.xdelta", "fgk.xdelta"] {
            let patch = fixture(name);
            let plain = plain(&patch).unwrap();
            assert!(matches!(plain, Cow::Owned(_)), "{name}");
            let out = oxidelta::vcdiff::decode_memory(&plain, &src).unwrap();
            assert!(out == tgt, "{name}");
            // Through the patcher's own entry point as well.
            assert!(
                crate::patch::apply(&patch, &src).as_deref() == Some(&tgt[..]),
                "{name}"
            );
        }
    }

    #[test]
    fn damaged_secondary_data_is_an_error() {
        let mut patch = fixture("djw.xdelta");
        let n = patch.len();
        patch.truncate(n - 40);
        assert!(plain(&patch).is_err());
    }
}
