//! A small PNG encoder, so the core can embed rasters it computes (the
//! glass layer of `Ink::Glass`) without a dependency.
//!
//! [`encode_rgba`] writes an 8-bit, non-interlaced PNG with an `sRGB`
//! chunk: colour type 6 (RGBA), or colour type 2 (RGB) when every pixel is
//! opaque. Each scanline gets the filter (None, Sub, Up, Average or Paeth)
//! with the smallest sum of absolute differences, and the filtered bytes are
//! compressed in-house: LZ77 over a 32 KB window with a hash-chain matcher
//! and lazy matching, then each block is written as dynamic Huffman
//! (code lengths limited to 15 bits, code-length codes to 7), fixed Huffman
//! or stored, whichever is smallest.
//!
//! The output is deterministic: the same pixels give the same bytes on
//! every platform.
//!
//! References: PNG (W3C, Third Edition), zlib (RFC 1950), DEFLATE
//! (RFC 1951). CRC-32 is the PNG/ISO 3309 polynomial; Adler-32 is RFC 1950's.

use crate::image::Rgba8Image;
use alloc::vec;
use alloc::vec::Vec;

/// Encodes `image` as a PNG file.
///
/// Fully opaque images are written as RGB (colour type 2), anything else as
/// RGBA (colour type 6); both decode to exactly the input pixels.
pub fn encode_rgba(image: &Rgba8Image) -> Vec<u8> {
    let (width, height, rgba) = (image.width(), image.height(), image.data());
    let opaque = rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255);
    let channels = if opaque { 3 } else { 4 };
    let raw = filter(width as usize, rgba, channels);

    let mut out = Vec::with_capacity(raw.len() / 2 + 64);
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = [0u8; 13];
    ihdr[..4].copy_from_slice(&width.to_be_bytes());
    ihdr[4..8].copy_from_slice(&height.to_be_bytes());
    ihdr[8] = 8; // bit depth
    ihdr[9] = if opaque { 2 } else { 6 }; // colour type
    // Compression, filter method and interlace stay 0.
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"sRGB", &[0]); // perceptual rendering intent
    chunk(&mut out, b"IDAT", &zlib(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Appends a PNG chunk: length, type, data, CRC of type and data.
fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    // Rgba8Image's size limits keep this far below 2³¹ for any image a
    // decoder would accept; PNG caps a chunk there.
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

// ---------------------------------------------------------------------------
// Scanline filters (PNG §7.3, §9)

/// The filtered image data: each row is its filter byte, then the row
/// filtered with whichever of the five filters scores lowest.
fn filter(width: usize, rgba: &[u8], channels: usize) -> Vec<u8> {
    let stride = width * channels;
    let rows = rgba.len() / (width * 4);
    let mut out = Vec::with_capacity(rows * (stride + 1));
    let mut prev = vec![0u8; stride];
    let mut cur = vec![0u8; stride];
    let mut cand: [Vec<u8>; 5] = core::array::from_fn(|_| vec![0u8; stride]);
    for row in rgba.chunks_exact(width * 4) {
        if channels == 4 {
            cur.copy_from_slice(row);
        } else {
            let (dst, src) = (cur.as_chunks_mut::<3>().0, row.as_chunks::<4>().0);
            for (d, s) in dst.iter_mut().zip(src) {
                d.copy_from_slice(&s[..3]);
            }
        }
        let mut best = (u64::MAX, 0);
        for (kind, c) in cand.iter_mut().enumerate() {
            let score = filter_row(kind as u8, &cur, &prev, channels, c);
            if score < best.0 {
                best = (score, kind);
            }
        }
        out.push(best.1 as u8);
        out.extend_from_slice(&cand[best.1]);
        core::mem::swap(&mut prev, &mut cur);
    }
    out
}

/// Filters one row with filter `kind` into `out`; returns the sum of the
/// filtered bytes read as signed magnitudes.
fn filter_row(kind: u8, cur: &[u8], prev: &[u8], bpp: usize, out: &mut [u8]) -> u64 {
    let mut score = 0u64;
    for i in 0..cur.len() {
        let a = if i >= bpp { cur[i - bpp] } else { 0 };
        let b = prev[i];
        let c = if i >= bpp { prev[i - bpp] } else { 0 };
        let predictor = match kind {
            0 => 0,
            1 => a,
            2 => b,
            3 => ((a as u16 + b as u16) / 2) as u8,
            _ => paeth(a, b, c),
        };
        let v = cur[i].wrapping_sub(predictor);
        out[i] = v;
        score += (v as i8).unsigned_abs() as u64;
    }
    score
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i16 + b as i16 - c as i16;
    let (pa, pb, pc) = (
        (p - a as i16).abs(),
        (p - b as i16).abs(),
        (p - c as i16).abs(),
    );
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

// ---------------------------------------------------------------------------
// Checksums

const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};

/// CRC-32 (ISO 3309, as PNG uses it).
fn crc32(data: &[u8]) -> u32 {
    !data.iter().fold(!0u32, |c, &b| {
        CRC_TABLE[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8)
    })
}

/// Adler-32 (RFC 1950 §8).
fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let (mut a, mut b) = (1u32, 0u32);
    // 5552 bytes is the most that can be summed before `b` overflows u32.
    for block in data.chunks(5552) {
        for &x in block {
            a += x as u32;
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    b << 16 | a
}

// ---------------------------------------------------------------------------
// zlib and DEFLATE (RFC 1950, RFC 1951)

/// `data` in a zlib stream.
fn zlib(data: &[u8]) -> Vec<u8> {
    // CM 8 (deflate), CINFO 7 (32 KB window), FLEVEL 3, FCHECK so the pair
    // is a multiple of 31.
    let mut w = BitWriter {
        out: vec![0x78, 0xda],
        acc: 0,
        n: 0,
    };
    deflate(data, &mut w);
    w.align();
    let mut out = w.out;
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

const WINDOW: usize = 1 << 15;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const HASH_BITS: u32 = 15;
const NONE: usize = usize::MAX;
/// Candidates examined per position.
const MAX_CHAIN: usize = 128;
/// After a match this long, search a quarter of the chain.
const GOOD_MATCH: usize = 16;
/// A match this long is taken without looking one byte further.
const MAX_LAZY: usize = 32;
/// A match this long ends the search.
const NICE_MATCH: usize = 128;
/// A three-byte match further back than this costs more than literals.
const TOO_FAR: usize = 4096;
/// Symbols buffered per block before it is written.
const BLOCK_SYMBOLS: usize = 1 << 15;
/// The most bytes one stored block holds.
const MAX_STORED: usize = 65_535;

/// One LZ77 symbol: a literal byte (`dist == 0`) or a match.
#[derive(Clone, Copy)]
struct Symbol {
    /// The literal, or the match length (3–258).
    len: u16,
    /// The match distance (1–32768), or 0 for a literal.
    dist: u16,
}

/// Hash chains over the input: `head` is the latest position per hash,
/// `prev` links each position (mod the window) to the one before it.
struct Matcher<'a> {
    data: &'a [u8],
    head: Vec<usize>,
    prev: Vec<usize>,
}

impl Matcher<'_> {
    fn hash(&self, i: usize) -> usize {
        let d = self.data;
        let v = (d[i] as u32) << 16 | (d[i + 1] as u32) << 8 | d[i + 2] as u32;
        (v.wrapping_mul(0x9e37_79b1) >> (32 - HASH_BITS)) as usize
    }

    /// Adds position `i` to its chain, if three bytes start there.
    fn insert(&mut self, i: usize) {
        if i + MIN_MATCH <= self.data.len() {
            let h = self.hash(i);
            self.prev[i % WINDOW] = self.head[h];
            self.head[h] = i;
        }
    }

    /// The longest match at `i` longer than `shorter_than`, as
    /// `(length, distance)`, or `(0, 0)`. Call before inserting `i`.
    fn longest(&self, i: usize, shorter_than: usize, mut chain: usize) -> (usize, usize) {
        let d = self.data;
        let max = MAX_MATCH.min(d.len() - i);
        let mut best = (shorter_than.max(MIN_MATCH - 1), 0);
        if max <= best.0 {
            return (0, 0);
        }
        let mut cand = self.head[self.hash(i)];
        while cand != NONE && chain > 0 && i - cand <= WINDOW {
            if d[cand + best.0] == d[i + best.0] {
                let len = match_len(&d[cand..cand + max], &d[i..i + max]);
                if len > best.0 {
                    best = (len, i - cand);
                    if len >= NICE_MATCH || len == max {
                        break;
                    }
                }
            }
            let next = self.prev[cand % WINDOW];
            // A slot overwritten by a newer position ends the chain.
            if next >= cand {
                break;
            }
            cand = next;
            chain -= 1;
        }
        if best.1 == 0 || (best.0 == MIN_MATCH && best.1 > TOO_FAR) {
            (0, 0)
        } else {
            best
        }
    }
}

/// How many leading bytes `a` and `b` (equal lengths) share.
fn match_len(a: &[u8], b: &[u8]) -> usize {
    let mut k = 0;
    for (x, y) in a.as_chunks::<8>().0.iter().zip(b.as_chunks::<8>().0) {
        let (x, y) = (u64::from_le_bytes(*x), u64::from_le_bytes(*y));
        if x != y {
            return k + ((x ^ y).trailing_zeros() / 8) as usize;
        }
        k += 8;
    }
    k + a[k..]
        .iter()
        .zip(&b[k..])
        .take_while(|(x, y)| x == y)
        .count()
}

/// Compresses `data` as DEFLATE blocks into `w`.
fn deflate(data: &[u8], w: &mut BitWriter) {
    let mut m = Matcher {
        data,
        head: vec![NONE; 1 << HASH_BITS],
        prev: vec![NONE; WINDOW],
    };
    let mut symbols: Vec<Symbol> = Vec::with_capacity(BLOCK_SYMBOLS + 1);
    let mut block_start = 0; // first input byte of the buffered block
    let mut pending: Option<(usize, usize)> = None; // a match found at i - 1
    let mut i = 0;
    while i < data.len() {
        let chain = match pending {
            Some((len, _)) if len >= GOOD_MATCH => MAX_CHAIN / 4,
            _ => MAX_CHAIN,
        };
        let (len, dist) = if i + MIN_MATCH <= data.len() {
            m.longest(i, pending.map_or(0, |p| p.0), chain)
        } else {
            (0, 0)
        };
        m.insert(i);
        let mut next = i + 1;
        match pending.take() {
            Some((plen, pdist)) if plen >= len => {
                // The match at i - 1 wins; it covers i - 1 .. i - 1 + plen.
                symbols.push(Symbol {
                    len: plen as u16,
                    dist: pdist as u16,
                });
                for j in i + 1..i - 1 + plen {
                    m.insert(j);
                }
                next = i - 1 + plen;
            }
            prior => {
                if prior.is_some() {
                    symbols.push(literal(data[i - 1]));
                }
                if len >= MAX_LAZY {
                    symbols.push(Symbol {
                        len: len as u16,
                        dist: dist as u16,
                    });
                    for j in i + 1..i + len {
                        m.insert(j);
                    }
                    next = i + len;
                } else if len >= MIN_MATCH {
                    pending = Some((len, dist));
                } else {
                    symbols.push(literal(data[i]));
                }
            }
        }
        i = next;
        if pending.is_none() && symbols.len() >= BLOCK_SYMBOLS {
            write_block(&symbols, &data[block_start..i], false, w);
            symbols.clear();
            block_start = i;
        }
    }
    write_block(&symbols, &data[block_start..], true, w);
}

fn literal(byte: u8) -> Symbol {
    Symbol {
        len: byte as u16,
        dist: 0,
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
/// Order in which code-length code lengths are sent (RFC 1951 §3.2.7).
const CL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// Length (3–258) → index into `LEN_BASE`.
const LEN_CODE: [u8; 259] = {
    let mut t = [0u8; 259];
    let mut code = 0;
    while code < 29 {
        let mut l = LEN_BASE[code] as usize;
        while l < LEN_BASE[code] as usize + (1 << LEN_EXTRA[code]) && l < 259 {
            t[l] = code as u8;
            l += 1;
        }
        code += 1;
    }
    t[258] = 28; // 258 has its own code, not 227 + 31
    t
};

/// Distance → index into `DIST_BASE`, via zlib's split table: `d - 1`
/// below 256 directly, larger distances by `(d - 1) >> 7`.
const DIST_CODE: [u8; 512] = {
    let mut t = [0u8; 512];
    let mut code = 0;
    while code < 30 {
        let mut d = DIST_BASE[code] as usize;
        while d < DIST_BASE[code] as usize + (1 << DIST_EXTRA[code]) {
            let idx = if d - 1 < 256 {
                d - 1
            } else {
                256 + ((d - 1) >> 7)
            };
            t[idx] = code as u8;
            d += 1;
        }
        code += 1;
    }
    t
};

fn dist_code(d: usize) -> usize {
    (if d <= 256 {
        DIST_CODE[d - 1]
    } else {
        DIST_CODE[256 + ((d - 1) >> 7)]
    }) as usize
}

/// A symbol in DEFLATE's alphabets.
struct Coded {
    /// Literal/length symbol (0–285).
    lit: usize,
    /// The rest of a match.
    tail: Option<Tail>,
}

/// A match's length extra bits, distance symbol and distance extra bits;
/// extra bits as `(value, count)`.
#[derive(Clone, Copy)]
struct Tail {
    len_extra: (u32, u32),
    dist: usize,
    dist_extra: (u32, u32),
}

impl Coded {
    fn extra_bits(&self) -> u64 {
        self.tail
            .map_or(0, |t| (t.len_extra.1 + t.dist_extra.1) as u64)
    }
}

fn split(s: Symbol) -> Coded {
    if s.dist == 0 {
        return Coded {
            lit: s.len as usize,
            tail: None,
        };
    }
    let lc = LEN_CODE[s.len as usize] as usize;
    let dc = dist_code(s.dist as usize);
    Coded {
        lit: 257 + lc,
        tail: Some(Tail {
            len_extra: ((s.len - LEN_BASE[lc]) as u32, LEN_EXTRA[lc] as u32),
            dist: dc,
            dist_extra: ((s.dist - DIST_BASE[dc]) as u32, DIST_EXTRA[dc] as u32),
        }),
    }
}

/// Writes one block, choosing dynamic Huffman, fixed Huffman or stored by
/// exact size. `raw` is the input the symbols encode.
fn write_block(symbols: &[Symbol], raw: &[u8], last: bool, w: &mut BitWriter) {
    let mut lit_freq = [0u32; 286];
    let mut dist_freq = [0u32; 30];
    // Bits shared by both Huffman kinds: the extra bits of every match.
    let mut extra = 0u64;
    for &s in symbols {
        let c = split(s);
        lit_freq[c.lit] += 1;
        if let Some(t) = c.tail {
            dist_freq[t.dist] += 1;
        }
        extra += c.extra_bits();
    }
    lit_freq[256] = 1;
    let cost = |freq: &[u32], lens: &[u8]| -> u64 {
        freq.iter()
            .zip(lens)
            .map(|(&f, &l)| f as u64 * l as u64)
            .sum()
    };

    let fixed_lit = fixed_lit_lengths();
    let fixed_dist = [5u8; 30];
    let fixed_bits = 3 + cost(&lit_freq, &fixed_lit) + cost(&dist_freq, &fixed_dist) + extra;

    let dynamic = Dynamic::new(&lit_freq, &dist_freq);
    let dynamic_bits = 3
        + dynamic.header_bits()
        + cost(&lit_freq, &dynamic.lit)
        + cost(&dist_freq, &dynamic.dist)
        + extra;

    // Stored: per piece of up to 65 535 bytes, 3 header bits, padding to a
    // byte, LEN and NLEN, then the bytes. Pieces after the first start
    // byte-aligned, so their header pads by 5.
    let pieces = raw.len().div_ceil(MAX_STORED).max(1);
    let pad = (8 - (w.n + 3) % 8) % 8;
    let stored_bits = (3 + pad as u64 + 32) + (pieces as u64 - 1) * (8 + 32) + 8 * raw.len() as u64;

    let last = last as u64;
    if stored_bits < fixed_bits.min(dynamic_bits) {
        for k in 0..pieces {
            let piece = &raw[k * MAX_STORED..raw.len().min((k + 1) * MAX_STORED)];
            w.bits(last & (k + 1 == pieces) as u64, 3); // BTYPE 00
            w.align();
            let n = piece.len() as u64;
            w.bits(n | (!n & 0xffff) << 16, 32);
            w.flush_bytes(piece);
        }
    } else if fixed_bits <= dynamic_bits {
        w.bits(last | 1 << 1, 3);
        let lit = Code::new(&fixed_lit);
        let dist = Code::new(&fixed_dist);
        write_symbols(symbols, &lit, &dist, w);
    } else {
        w.bits(last | 2 << 1, 3);
        dynamic.write_header(w);
        let lit = Code::new(&dynamic.lit);
        let dist = Code::new(&dynamic.dist);
        write_symbols(symbols, &lit, &dist, w);
    }
}

fn write_symbols(symbols: &[Symbol], lit: &Code, dist: &Code, w: &mut BitWriter) {
    for &s in symbols {
        let c = split(s);
        lit.put(c.lit, w);
        if let Some(t) = c.tail {
            w.bits(t.len_extra.0 as u64, t.len_extra.1);
            dist.put(t.dist, w);
            w.bits(t.dist_extra.0 as u64, t.dist_extra.1);
        }
    }
    lit.put(256, w);
}

fn fixed_lit_lengths() -> [u8; 288] {
    let mut l = [8u8; 288];
    l[144..256].fill(9);
    l[256..280].fill(7);
    l
}

/// A dynamic block's code lengths and its run-length-coded header.
struct Dynamic {
    lit: Vec<u8>,
    dist: Vec<u8>,
    /// Code-length symbols with their extra bits: `(symbol, extra value)`.
    runs: Vec<(u8, u8)>,
    cl: Vec<u8>,
    hlit: usize,
    hdist: usize,
    hclen: usize,
}

impl Dynamic {
    fn new(lit_freq: &[u32; 286], dist_freq: &[u32; 30]) -> Dynamic {
        let lit = code_lengths(lit_freq, 15);
        let dist = code_lengths(dist_freq, 15);
        let hlit = 257.max(286 - lit.iter().rev().take_while(|&&l| l == 0).count());
        let hdist = 1.max(30 - dist.iter().rev().take_while(|&&l| l == 0).count());
        let all: Vec<u8> = lit[..hlit].iter().chain(&dist[..hdist]).copied().collect();
        let runs = run_lengths(&all);
        let mut cl_freq = [0u32; 19];
        for &(s, _) in &runs {
            cl_freq[s as usize] += 1;
        }
        let cl = code_lengths(&cl_freq, 7);
        let hclen = 4.max(19 - CL_ORDER.iter().rev().take_while(|&&s| cl[s] == 0).count());
        Dynamic {
            lit,
            dist,
            runs,
            cl,
            hlit,
            hdist,
            hclen,
        }
    }

    fn header_bits(&self) -> u64 {
        let runs: u64 = self
            .runs
            .iter()
            .map(|&(s, _)| self.cl[s as usize] as u64 + run_extra(s) as u64)
            .sum();
        14 + 3 * self.hclen as u64 + runs
    }

    fn write_header(&self, w: &mut BitWriter) {
        w.bits((self.hlit - 257) as u64, 5);
        w.bits((self.hdist - 1) as u64, 5);
        w.bits((self.hclen - 4) as u64, 4);
        for &s in &CL_ORDER[..self.hclen] {
            w.bits(self.cl[s] as u64, 3);
        }
        let code = Code::new(&self.cl);
        for &(s, x) in &self.runs {
            code.put(s as usize, w);
            w.bits(x as u64, run_extra(s));
        }
    }
}

/// Extra bits after a code-length symbol: 16 repeats, 17 and 18 zero runs.
fn run_extra(symbol: u8) -> u32 {
    match symbol {
        16 => 2,
        17 => 3,
        18 => 7,
        _ => 0,
    }
}

/// Run-length codes the concatenated code lengths (RFC 1951 §3.2.7).
fn run_lengths(lens: &[u8]) -> Vec<(u8, u8)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lens.len() {
        let v = lens[i];
        let mut n = lens[i..].iter().take_while(|&&l| l == v).count();
        i += n;
        if v == 0 {
            while n >= 11 {
                let k = n.min(138);
                out.push((18, (k - 11) as u8));
                n -= k;
            }
            if n >= 3 {
                out.push((17, (n - 3) as u8));
                n = 0;
            }
        } else {
            out.push((v, 0));
            n -= 1;
            while n >= 3 {
                let k = n.min(6);
                out.push((16, (k - 3) as u8));
                n -= k;
            }
        }
        out.extend(core::iter::repeat_n((v, 0), n));
    }
    out
}

/// Huffman code lengths for `freq`, none longer than `limit`. Always
/// returns a complete code: at least two symbols get a length, so inflaters
/// that reject one-symbol trees accept it.
fn code_lengths(freq: &[u32], limit: u8) -> Vec<u8> {
    let mut freq = freq.to_vec();
    for s in 0..2 {
        if freq.iter().filter(|&&f| f > 0).count() < 2 && freq[s] == 0 {
            freq[s] = 1;
        }
    }
    // Leaves by ascending frequency, ties by symbol: deterministic.
    let mut leaves: Vec<(u32, usize)> = freq
        .iter()
        .enumerate()
        .filter(|&(_, &f)| f > 0)
        .map(|(s, &f)| (f, s))
        .collect();
    leaves.sort_unstable();
    let n = leaves.len();

    // Two-queue Huffman: leaves are nodes 0..n, internal nodes n..2n-1 in
    // creation order, so a parent always has a larger index than its child.
    let mut weight: Vec<u64> = leaves.iter().map(|&(f, _)| f as u64).collect();
    let mut parent = vec![0usize; 2 * n - 1];
    let (mut next_leaf, mut next_node) = (0, n);
    for node in n..2 * n - 1 {
        let mut pick = || {
            let take_leaf =
                next_leaf < n && (next_node >= node || weight[next_leaf] <= weight[next_node]);
            if take_leaf {
                next_leaf += 1;
                next_leaf - 1
            } else {
                next_node += 1;
                next_node - 1
            }
        };
        let (a, b) = (pick(), pick());
        weight.push(weight[a] + weight[b]);
        parent[a] = node;
        parent[b] = node;
    }
    let mut depth = vec![0u8; 2 * n - 1];
    for i in (0..2 * n - 2).rev() {
        depth[i] = depth[parent[i]] + 1;
    }

    // Count codes per length, fold anything over the limit into it, then
    // repay the Kraft overdraft by lengthening shorter codes.
    let mut count = [0u32; 64];
    for &d in &depth[..n] {
        count[d as usize] += 1;
    }
    let limit = limit as usize;
    for l in limit + 1..64 {
        count[limit] += count[l];
        count[l] = 0;
    }
    let mut kraft: u64 = (1..=limit).map(|l| (count[l] as u64) << (limit - l)).sum();
    while kraft > 1 << limit {
        count[limit] -= 1;
        if let Some(l) = (1..limit).rev().find(|&l| count[l] > 0) {
            count[l] -= 1;
            count[l + 1] += 2;
        }
        kraft -= 1;
    }

    // Longest codes to the rarest symbols.
    let mut lens = vec![0u8; freq.len()];
    let mut it = leaves.iter();
    for l in (1..=limit).rev() {
        for _ in 0..count[l] {
            if let Some(&(_, s)) = it.next() {
                lens[s] = l as u8;
            }
        }
    }
    lens
}

/// Canonical Huffman codes (RFC 1951 §3.2.2), bit-reversed for the
/// LSB-first stream.
struct Code {
    codes: Vec<u16>,
    lens: Vec<u8>,
}

impl Code {
    fn new(lens: &[u8]) -> Code {
        let mut count = [0u16; 16];
        for &l in lens {
            count[l as usize] += 1;
        }
        count[0] = 0;
        let mut next = [0u16; 16];
        let mut code = 0u16;
        for bits in 1..16 {
            code = (code + count[bits - 1]) << 1;
            next[bits] = code;
        }
        let codes = lens
            .iter()
            .map(|&l| {
                if l == 0 {
                    return 0;
                }
                let c = next[l as usize];
                next[l as usize] += 1;
                c.reverse_bits() >> (16 - l)
            })
            .collect();
        Code {
            codes,
            lens: lens.to_vec(),
        }
    }

    fn put(&self, symbol: usize, w: &mut BitWriter) {
        w.bits(self.codes[symbol] as u64, self.lens[symbol] as u32);
    }
}

/// Packs bits LSB-first, as DEFLATE wants.
struct BitWriter {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl BitWriter {
    /// Appends the low `n` bits of `v` (`n` ≤ 32).
    fn bits(&mut self, v: u64, n: u32) {
        self.acc |= (v & ((1u64 << n) - 1)) << self.n;
        self.n += n;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    /// Appends whole bytes; the writer must be byte-aligned.
    fn flush_bytes(&mut self, bytes: &[u8]) {
        debug_assert_eq!(self.n, 0);
        self.out.extend_from_slice(bytes);
    }

    /// Pads with zero bits to a byte boundary.
    fn align(&mut self) {
        if self.n > 0 {
            self.bits(0, 8 - self.n);
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::rng::SplitMix64;
    use std::time::Instant;
    use std::{println, vec};

    /// Decodes `bytes` with the `png` crate, expanding RGB to RGBA.
    fn decode(bytes: &[u8]) -> (u32, u32, Vec<u8>, ::png::ColorType) {
        let mut reader = ::png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .expect("valid PNG header");
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).expect("valid PNG data");
        buf.truncate(info.buffer_size());
        let rgba = match info.color_type {
            ::png::ColorType::Rgba => buf,
            ::png::ColorType::Rgb => buf
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            other => panic!("unexpected colour type {other:?}"),
        };
        (info.width, info.height, rgba, info.color_type)
    }

    fn round_trip(img: &Rgba8Image) -> Vec<u8> {
        let bytes = encode_rgba(img);
        let (w, h, rgba, _) = decode(&bytes);
        assert_eq!((w, h), (img.width(), img.height()));
        assert!(rgba == img.data(), "pixels differ at {w}×{h}");
        bytes
    }

    fn image(w: u32, h: u32, mut f: impl FnMut(u32, u32) -> [u8; 4]) -> Rgba8Image {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&f(x, y));
            }
        }
        Rgba8Image::new(w, h, data).unwrap()
    }

    fn noise(w: u32, h: u32, seed: u64) -> Rgba8Image {
        let mut rng = SplitMix64::new(seed);
        let data = (0..w * h).flat_map(|_| rng.next_u64().to_le_bytes()[..4].to_vec());
        Rgba8Image::new(w, h, data.collect()).unwrap()
    }

    /// Smooth shading with soft edges and a little grain, in integers: a
    /// stand-in for a photograph.
    fn photo(side: u32) -> Rgba8Image {
        let mut rng = SplitMix64::new(7);
        let s = side as i64;
        image(side, side, |x, y| {
            let (x, y) = (x as i64, y as i64);
            let grain = (rng.next_u64() % 7) as i64 - 3;
            let dx = x - s / 3;
            let dy = y - s / 2;
            let blob = (s * s / 9 - (dx * dx + dy * dy)).max(0) * 120 / (s * s / 9);
            let wave = ((x * 7 + y * 3) % (s / 2) - s / 4).abs() * 160 / (s / 4);
            let r = 40 + x * 150 / s + blob + grain;
            let g = 30 + wave / 2 + y * 60 / s + grain;
            let b = 200 - y * 120 / s + blob / 3 + grain;
            [clamp(r), clamp(g), clamp(b), 255]
        })
    }

    fn clamp(v: i64) -> u8 {
        v.clamp(0, 255) as u8
    }

    /// The glass layer's shape: transparent, with soft-edged blobs on a
    /// module grid.
    fn sparse(side: u32) -> Rgba8Image {
        image(side, side, |x, y| {
            let (cx, cy) = (x % 24, y % 24);
            let d2 = (cx as i32 - 12).pow(2) + (cy as i32 - 12).pow(2);
            if (x / 24 + 3 * (y / 24)) % 5 == 0 && d2 < 64 {
                let a = (255 - d2 * 3).clamp(0, 255) as u8;
                [230, 240, 250, a]
            } else {
                [0, 0, 0, 0]
            }
        })
    }

    #[test]
    fn crc32_known_answers() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(
            crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414f_a339
        );
        // IEND's CRC, as every PNG ends with it.
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
    }

    #[test]
    fn adler32_known_answers() {
        assert_eq!(adler32(b""), 1);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
        assert_eq!(adler32(b"123456789"), 0x091e_01de);
        // Long enough to cross several 5552-byte blocks.
        assert_eq!(adler32(&[0xff; 100_000]), 0x149a_302c);
    }

    #[test]
    fn code_lengths_stay_complete_and_limited() {
        // Fibonacci frequencies make a Huffman tree as deep as possible.
        let mut fib = [0u32; 30];
        let (mut a, mut b) = (1u32, 1u32);
        for f in &mut fib {
            *f = a;
            (a, b) = (b, a + b);
        }
        for (freq, limit) in [(&fib[..], 15u8), (&fib[..19], 7), (&[5, 0, 0][..], 15)] {
            let lens = code_lengths(freq, limit);
            assert!(lens.iter().all(|&l| l <= limit));
            let kraft: u64 = lens
                .iter()
                .filter(|&&l| l > 0)
                .map(|&l| 1u64 << (limit - l))
                .sum();
            assert_eq!(kraft, 1 << limit, "complete code for {freq:?}");
        }
    }

    #[test]
    fn length_and_distance_tables_agree() {
        for len in 3..=258u16 {
            let c = LEN_CODE[len as usize] as usize;
            assert!(LEN_BASE[c] <= len && len - LEN_BASE[c] < 1 << LEN_EXTRA[c]);
        }
        for d in 1..=32_768usize {
            let c = dist_code(d);
            assert!(DIST_BASE[c] as usize <= d && d - (DIST_BASE[c] as usize) < 1 << DIST_EXTRA[c]);
        }
    }

    #[test]
    fn round_trips_small_and_odd_sizes() {
        round_trip(&Rgba8Image::filled(1, 1, [10, 20, 30, 40]).unwrap());
        round_trip(&Rgba8Image::filled(1, 1, [10, 20, 30, 255]).unwrap());
        for (w, h) in [(1, 7), (7, 1), (3, 5), (17, 13), (101, 3), (255, 257)] {
            round_trip(&noise(w, h, w as u64 * 1000 + h as u64));
            round_trip(&image(w, h, |x, y| [x as u8, y as u8, (x ^ y) as u8, 255]));
        }
    }

    #[test]
    fn round_trips_transparent_and_opaque() {
        let clear = round_trip(&Rgba8Image::filled(64, 48, [0, 0, 0, 0]).unwrap());
        assert_eq!(clear[25], 6, "transparent stays RGBA");
        let solid = round_trip(&Rgba8Image::filled(64, 48, [9, 99, 199, 255]).unwrap());
        assert_eq!(solid[25], 2, "opaque becomes RGB");
        // One translucent pixel keeps the alpha channel.
        let mut img = Rgba8Image::filled(5, 5, [1, 2, 3, 255]).unwrap();
        img.set_pixel(4, 4, [1, 2, 3, 254]);
        assert_eq!(round_trip(&img)[25], 6);
        // Colour under zero alpha survives too.
        round_trip(&image(9, 9, |x, y| [x as u8 * 20, y as u8 * 20, 7, 0]));
    }

    #[test]
    fn round_trips_gradients_noise_sparse_and_photo() {
        round_trip(&image(300, 200, |x, y| {
            [x as u8, y as u8, (x + y) as u8 / 2, (255 - y) as u8]
        }));
        round_trip(&noise(256, 256, 0x5eed));
        round_trip(&sparse(333));
        round_trip(&photo(512));
        // Long runs past the 258-byte match limit and the 65 535-byte
        // stored-block limit.
        round_trip(&Rgba8Image::filled(1000, 70, [5, 5, 5, 5]).unwrap());
    }

    #[test]
    fn output_is_deterministic_and_pinned() {
        let img = photo(128);
        assert_eq!(encode_rgba(&img), encode_rgba(&img));
        let tiny = image(3, 2, |x, y| {
            [x as u8 * 100, y as u8 * 200, 50, 128 + x as u8]
        });
        let bytes = encode_rgba(&tiny);
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(bytes.len(), PINNED_LEN, "{bytes:02x?}");
        assert_eq!(crc32(&bytes), PINNED_CRC, "{bytes:02x?}");
    }

    // Checked independently: Python's zlib inflates the IDAT to the
    // expected filtered rows (Sub, then Paeth).
    const PINNED_LEN: usize = 94;
    const PINNED_CRC: u32 = 0x4977_af21;

    /// What the `png` crate writes at its default settings (what
    /// `tiny-skia`'s `encode_png` uses), in the given colour type.
    fn reference(img: &Rgba8Image, rgb: bool) -> usize {
        let mut out = Vec::new();
        let mut enc = ::png::Encoder::new(&mut out, img.width(), img.height());
        enc.set_depth(::png::BitDepth::Eight);
        let data: Vec<u8> = if rgb {
            enc.set_color(::png::ColorType::Rgb);
            img.data()
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect()
        } else {
            enc.set_color(::png::ColorType::Rgba);
            img.data().to_vec()
        };
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&data).unwrap();
        w.finish().unwrap();
        out.len()
    }

    #[test]
    fn sizes_stay_near_the_reference_encoder() {
        let photo = photo(512);
        let ours = encode_rgba(&photo).len();
        let (rgb, rgba) = (reference(&photo, true), reference(&photo, false));
        println!("photo 512²: ours {ours} B, png crate RGB {rgb} B, RGBA {rgba} B");
        assert!(ours * 100 <= rgb * 115, "{ours} vs {rgb}");

        let sparse = sparse(512);
        let ours = encode_rgba(&sparse).len();
        let theirs = reference(&sparse, false);
        println!("sparse 512²: ours {ours} B, png crate {theirs} B");
        assert!(ours * 100 <= theirs * 115, "{ours} vs {theirs}");

        // Noise can't compress; stored blocks cap the growth.
        let noise = noise(256, 256, 1);
        let ours = encode_rgba(&noise).len();
        let raw = 256 * (256 * 4 + 1);
        // Signature and chunk framing, the zlib wrapper, then 5 bytes per
        // stored block (at most one per buffered block, plus the last).
        let overhead = 8 + 25 + 13 + 12 + 12 + 6 + (raw / BLOCK_SYMBOLS + 1) * 5;
        println!(
            "noise 256²: ours {ours} B, raw {raw} B, png crate {}",
            reference(&noise, false)
        );
        assert!(ours <= raw + overhead, "{ours} > {raw} + {overhead}");
    }

    /// Run with `cargo test --release -p freeqrent --lib png -- --ignored
    /// --nocapture` to time the encoder.
    #[test]
    #[ignore]
    fn timing() {
        for side in [1024, 2048] {
            for (name, img) in [
                ("photo", photo(side)),
                ("sparse", sparse(side)),
                ("noise", noise(side, side, 3)),
            ] {
                let t = Instant::now();
                let ours = encode_rgba(&img);
                let ours_t = t.elapsed();
                let t = Instant::now();
                let theirs = reference(&img, name == "photo");
                let theirs_t = t.elapsed();
                println!(
                    "{name} {side}²: ours {} B in {ours_t:?}, png crate {theirs} B in {theirs_t:?}",
                    ours.len()
                );
            }
        }
    }
}
