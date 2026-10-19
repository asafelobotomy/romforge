// Ported from xdelta3 (`xdelta3-djw.h`, `xdelta3-fgk.h`,
// `xdelta3-second.h`), Copyright 2016 Joshua MacDonald, licensed under the
// Apache License, Version 2.0 (<http://www.apache.org/licenses/LICENSE-2.0>);
// this file is a modified version of those parts (translated to Rust,
// decoding only) under the same licence.

use super::Res;
use super::djw::ALPHABET;

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

pub(super) struct Fgk {
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
    pub(super) fn new() -> Self {
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

    pub(super) fn decode(&mut self, input: &[u8], out_len: usize) -> Res<Vec<u8>> {
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
