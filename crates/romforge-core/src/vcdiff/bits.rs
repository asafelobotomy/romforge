// Ported from xdelta3 (`xdelta3-djw.h`, `xdelta3-fgk.h`,
// `xdelta3-second.h`), Copyright 2016 Joshua MacDonald, licensed under the
// Apache License, Version 2.0 (<http://www.apache.org/licenses/LICENSE-2.0>);
// this file is a modified version of those parts (translated to Rust,
// decoding only) under the same licence.

use super::Res;

// ------------------------------------------------------------------------
// Bits (xdelta3-second.h): least significant bit of each byte first.

pub(super) struct Bits<'a> {
    input: &'a [u8],
    pos: usize,
    pub(super) cur_byte: u32,
    pub(super) cur_mask: u32,
}

impl<'a> Bits<'a> {
    pub(super) fn new(input: &'a [u8]) -> Self {
        Bits {
            input,
            pos: 0,
            cur_byte: 0,
            cur_mask: 0x100,
        }
    }

    pub(super) fn next_byte(&mut self) -> Res<()> {
        self.cur_byte = u32::from(*self.input.get(self.pos).ok_or("secondary decoder end of input")?);
        self.pos += 1;
        self.cur_mask = 1;
        Ok(())
    }

    /// `nbits` bits, the first read the most significant.
    pub(super) fn read(&mut self, nbits: u32) -> Res<usize> {
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
