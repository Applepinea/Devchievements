//! Minimal zlib/DEFLATE decoder.
//!
//! WHY HAND-ROLLED: the Git Hook Helper must read a commit object with **zero
//! external crates** so it can be linked into a single small native binary with
//! no build-time surprises (ADR-0002). Git stores loose objects as ordinary zlib
//! streams, so all we need is a correct RFC1950/RFC1951 decoder.
//!
//! Supported: stored blocks, fixed-Huffman blocks, dynamic-Huffman blocks.
//! Not supported (and not needed for loose commit objects): preset dictionaries,
//! multi-member streams, gzip wrappers.
//!
//! This code was validated against real \`git\` output before being adopted; the
//! first two versions were wrong and the correctness assertion caught them.

/// Inflate a zlib (RFC1950) stream.
pub fn zlib_inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 2 {
        return Err("too short".into());
    }
    let cmf = data[0];
    let flg = data[1];
    if cmf & 0x0f != 8 {
        return Err("not deflate".into());
    }
    if (u16::from(cmf) * 256 + u16::from(flg)) % 31 != 0 {
        return Err("bad zlib header".into());
    }
    let mut pos = 2usize;
    if flg & 0x20 != 0 {
        pos += 4; // preset dictionary
    }
    inflate_deflate(data, pos)
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
    acc: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8], pos: usize) -> Self {
        BitReader {
            data,
            pos,
            bit: 0,
            acc: 0,
        }
    }
    fn need(&mut self) -> Result<(), String> {
        while self.bit < 24 {
            if self.pos >= self.data.len() {
                return Err("unexpected end of deflate stream".into());
            }
            self.acc |= u32::from(self.data[self.pos]) << self.bit;
            self.pos += 1;
            self.bit += 8;
        }
        Ok(())
    }
    fn bits(&mut self, n: u32) -> Result<u32, String> {
        if n == 0 {
            return Ok(0);
        }
        self.need()?;
        let v = self.acc & ((1u32 << n) - 1);
        self.acc >>= n;
        self.bit -= n;
        Ok(v)
    }
    fn align(&mut self) {
        let drop = self.bit % 8;
        self.acc >>= drop;
        self.bit -= drop;
    }
}

/// A Huffman decoding table: for every symbol, its canonical code and length.
#[derive(Clone)]
struct Huffman {
    codes: Vec<u16>,
    lens: Vec<u8>,
    max_len: u8,
}

impl Huffman {
    /// Build canonical codes from code lengths (RFC1951 3.2.2).
    fn from_lengths(lens: &[u8]) -> Result<Self, String> {
        let max_len = *lens.iter().max().unwrap_or(&0);
        if max_len == 0 {
            return Err("empty huffman table".into());
        }
        let mut bl_count = [0u32; 16];
        for &l in lens {
            if l > 15 {
                return Err("code length too large".into());
            }
            bl_count[l as usize] += 1;
        }
        bl_count[0] = 0;

        let mut next_code = [0u32; 16];
        let mut code = 0u32;
        for bits in 1..=15usize {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }

        // The decoder below consumes one bit at a time and shifts it in from the
        // right, which reconstructs the code MSB-first. That is exactly how
        // DEFLATE packs Huffman codes, so we store the canonical code UNREVERSED.
        // (An earlier version bit-reversed them, which breaks every multi-bit
        // code -- the correctness check caught it.)
        let mut codes = vec![0u16; lens.len()];
        for (sym, &l) in lens.iter().enumerate() {
            if l == 0 {
                continue;
            }
            let c = next_code[l as usize];
            next_code[l as usize] += 1;
            codes[sym] = c as u16;
        }
        Ok(Huffman {
            codes,
            lens: lens.to_vec(),
            max_len,
        })
    }

    /// Decode one symbol, reading bits LSB-first as DEFLATE requires.
    fn decode(&self, br: &mut BitReader<'_>) -> Result<u16, String> {
        let mut code = 0u16;
        for len in 1..=self.max_len {
            code = (code << 1) | br.bits(1)? as u16;
            for (sym, (&c, &l)) in self.codes.iter().zip(self.lens.iter()).enumerate() {
                if l == len && c == code {
                    return Ok(sym as u16);
                }
            }
        }
        Err("invalid huffman code".into())
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn inflate_deflate(data: &[u8], pos: usize) -> Result<Vec<u8>, String> {
    let mut br = BitReader::new(data, pos);
    let mut out: Vec<u8> = Vec::new();

    loop {
        let last = br.bits(1)?;
        let btype = br.bits(2)?;
        match btype {
            0 => {
                br.align();
                let len = br.bits(16)? as usize;
                let _nlen = br.bits(16)?;
                for _ in 0..len {
                    out.push(br.bits(8)? as u8);
                }
            }
            1 => {
                let (lit, dist) = fixed_tables()?;
                inflate_block(&mut br, &lit, &dist, &mut out)?;
            }
            2 => {
                let (lit, dist) = dynamic_tables(&mut br)?;
                inflate_block(&mut br, &lit, &dist, &mut out)?;
            }
            _ => return Err("bad block type".into()),
        }
        if last == 1 {
            break;
        }
    }
    Ok(out)
}

fn inflate_block(
    br: &mut BitReader<'_>,
    lit: &Huffman,
    dist: &Huffman,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    loop {
        let sym = lit.decode(br)?;
        if sym < 256 {
            out.push(sym as u8);
        } else if sym == 256 {
            return Ok(());
        } else {
            let idx = (sym - 257) as usize;
            if idx >= LEN_BASE.len() {
                return Err("bad length symbol".into());
            }
            let len = LEN_BASE[idx] as usize + br.bits(LEN_EXTRA[idx] as u32)? as usize;
            let dsym = dist.decode(br)? as usize;
            if dsym >= DIST_BASE.len() {
                return Err("bad distance symbol".into());
            }
            let distance = DIST_BASE[dsym] as usize + br.bits(DIST_EXTRA[dsym] as u32)? as usize;
            if distance > out.len() {
                return Err("distance beyond output".into());
            }
            let start = out.len() - distance;
            for i in 0..len {
                let b = out[start + i];
                out.push(b);
            }
        }
    }
}

fn fixed_tables() -> Result<(Huffman, Huffman), String> {
    let mut lit_lens = vec![0u8; 288];
    for (i, l) in lit_lens.iter_mut().enumerate() {
        *l = if i < 144 {
            8
        } else if i < 256 {
            9
        } else if i < 280 {
            7
        } else {
            8
        };
    }
    let dist_lens = vec![5u8; 30];
    Ok((
        Huffman::from_lengths(&lit_lens)?,
        Huffman::from_lengths(&dist_lens)?,
    ))
}

/// Read a dynamic block's Huffman tables (RFC1951 3.2.7).
fn dynamic_tables(br: &mut BitReader<'_>) -> Result<(Huffman, Huffman), String> {
    let hlit = br.bits(5)? as usize + 257;
    let hdist = br.bits(5)? as usize + 1;
    let hclen = br.bits(4)? as usize + 4;

    // The code-length alphabet is stored in this scrambled order.
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let mut cl_lens = vec![0u8; 19];
    for i in 0..hclen {
        cl_lens[ORDER[i]] = br.bits(3)? as u8;
    }
    let cl_huff = Huffman::from_lengths(&cl_lens)?;

    // Decode the literal/length and distance code lengths.
    let mut lens: Vec<u8> = Vec::with_capacity(hlit + hdist);
    while lens.len() < hlit + hdist {
        let sym = cl_huff.decode(br)?;
        match sym {
            0..=15 => lens.push(sym as u8),
            16 => {
                let prev = *lens.last().ok_or("repeat with no previous length")?;
                let repeat = 3 + br.bits(2)? as usize;
                for _ in 0..repeat {
                    lens.push(prev);
                }
            }
            17 => {
                let repeat = 3 + br.bits(3)? as usize;
                lens.resize(lens.len() + repeat, 0);
            }
            18 => {
                let repeat = 11 + br.bits(7)? as usize;
                lens.resize(lens.len() + repeat, 0);
            }
            _ => return Err("bad code-length symbol".into()),
        }
    }
    if lens.len() > hlit + hdist {
        return Err("code lengths overran".into());
    }

    let lit_lens = &lens[..hlit];
    let dist_lens = &lens[hlit..hlit + hdist];

    // A distance table with no codes is legal; substitute a single dummy code so
    // construction cannot fail when the block only contains literals.
    let lit = Huffman::from_lengths(lit_lens)?;
    let dist = if dist_lens.iter().any(|&l| l > 0) {
        Huffman::from_lengths(dist_lens)?
    } else {
        Huffman::from_lengths(&[1u8])?
    };
    Ok((lit, dist))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_zlib_header() {
        // 0x00 0x00 is not a deflate stream.
        assert!(zlib_inflate(&[0x00, 0x00]).is_err());
    }

    #[test]
    fn rejects_truncated_header() {
        assert!(zlib_inflate(&[0x78]).is_err());
    }

    /// Reference vectors produced by a known-good zlib implementation
    /// (Node's \`zlib.deflateSync\`), NOT by hand.
    ///
    /// This matters: the first version of this test built the stored-block bytes
    /// by hand, got the BFINAL/BTYPE byte wrong, and "failed" against correct
    /// code. Test vectors must come from an independent implementation.
    const STORED_HI: [u8; 13] = [
        0x78, 0x01, 0x01, 0x02, 0x00, 0xfd, 0xff, 0x68, 0x69, 0x01, 0x3b, 0x00, 0xd2,
    ];

    const STORED_EMPTY: [u8; 11] = [
        0x78, 0x01, 0x01, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01,
    ];

    /// Fixed/dynamic Huffman, because 'hi' at default level uses Huffman coding.
    const HUFFMAN_HI: [u8; 10] = [0x78, 0x9c, 0xcb, 0xc8, 0x04, 0x00, 0x01, 0x3b, 0x00, 0xd2];

    #[test]
    fn decodes_a_reference_stored_block() {
        let out = zlib_inflate(&STORED_HI).expect("stored block must decode");
        assert_eq!(out, b"hi");
    }

    #[test]
    fn decodes_an_empty_reference_stored_block() {
        let out = zlib_inflate(&STORED_EMPTY).expect("empty stored block must decode");
        assert_eq!(out, b"");
    }

    #[test]
    fn decodes_a_reference_huffman_block() {
        let out = zlib_inflate(&HUFFMAN_HI).expect("huffman block must decode");
        assert_eq!(out, b"hi");
    }
}
