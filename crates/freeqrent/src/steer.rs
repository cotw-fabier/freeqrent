//! The free-bit solver (SPEC §3.1, §7.7 A): decoders stop reading at the
//! terminator, so every data bit after it is free. Reed–Solomon is linear
//! over GF(2), so each module of a block is the XOR of some of the block's
//! free bits and a constant. Choosing the free bits in priority order with
//! Gauss–Jordan elimination (Cox's QArt) makes as many wanted modules as
//! possible take the value asked for.

use alloc::vec;
use alloc::vec::Vec;

use crate::gf;
use crate::matrix::{self, Mask};
use crate::version::{BlockLayout, Ecc, Version};

/// What a module should look like after masking. Priority 0 means the
/// module doesn't matter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Want {
    pub(crate) dark: bool,
    pub(crate) priority: u32,
}

/// One Reed–Solomon block's bits: data then ECC, most significant bit
/// first, as a bit vector.
#[derive(Clone, Debug)]
struct Bits {
    words: Vec<u64>,
}

impl Bits {
    fn zeros(len: usize) -> Bits {
        Bits {
            words: vec![0; len.div_ceil(64)],
        }
    }

    fn get(&self, i: usize) -> bool {
        self.words[i / 64] >> (i % 64) & 1 != 0
    }

    fn set(&mut self, i: usize, v: bool) {
        let bit = 1u64 << (i % 64);
        if v {
            self.words[i / 64] |= bit;
        } else {
            self.words[i / 64] &= !bit;
        }
    }

    fn from_bytes(bytes: &[u8]) -> Bits {
        let mut bits = Bits::zeros(bytes.len() * 8);
        for (i, &b) in bytes.iter().enumerate() {
            for j in 0..8 {
                bits.set(i * 8 + j, b >> (7 - j) & 1 != 0);
            }
        }
        bits
    }

    fn byte(&self, i: usize) -> u8 {
        (0..8).fold(0, |acc, j| acc << 1 | self.get(i * 8 + j) as u8)
    }
}

/// A block after elimination. Each pivot flips its wanted bit `at` and
/// no earlier pivot's, so deciding the pivots in order settles each one
/// for good.
struct Block {
    /// Data codewords, then ECC.
    base: Bits,
    data_len: usize,
    pivots: Vec<(usize, Bits)>,
    /// Where each of the block's bits sits, as a module index.
    modules: Vec<usize>,
}

impl Block {
    /// The block's bits once each pivot, in order, has been applied if its
    /// bit is still wrong under `mask`.
    fn solve(&self, mask: Mask, wants: &[Want], size: usize) -> Bits {
        let mut bits = self.base.clone();
        for (at, flip) in &self.pivots {
            let m = self.modules[*at];
            let want = wants[m].dark ^ mask.inverts(m % size, m / size);
            if bits.get(*at) != want {
                for (w, f) in bits.words.iter_mut().zip(&flip.words) {
                    *w ^= f;
                }
            }
        }
        bits
    }
}

/// Priority-weighted agreement of `bits` with the wanted modules.
fn score(block: &Block, bits: &Bits, mask: Mask, wants: &[Want], size: usize) -> u64 {
    let mut score = 0;
    for (i, &m) in block.modules.iter().enumerate() {
        let want = wants[m];
        if want.priority > 0 && bits.get(i) ^ mask.inverts(m % size, m / size) == want.dark {
            score += want.priority as u64;
        }
    }
    score
}

/// Rewrites the free bits of `data` (the data codewords in block order,
/// bits from `free_from` on) so that modules match `wants` (row-major, one
/// per module) as well as they can, and returns the mask that matches best,
/// or `mask` when one is forced.
pub(crate) fn steer(
    version: Version,
    ecc: Ecc,
    data: &mut [u8],
    free_from: usize,
    wants: &[Want],
    mask: Option<Mask>,
) -> Mask {
    let layout = BlockLayout::new(version, ecc);
    let size = version.size() as usize;
    let e = layout.ecc_per_block;
    let short = layout.num_short_blocks();
    let short_len = layout.short_block_data_len();
    let lens: Vec<usize> = (0..layout.num_blocks)
        .map(|b| short_len + usize::from(b >= short))
        .collect();
    let starts: Vec<usize> = lens
        .iter()
        .scan(0, |at, &len| {
            let s = *at;
            *at += len;
            Some(s)
        })
        .collect();

    // Which module each block bit lands in, through the interleaving.
    let placed = matrix::bit_modules(version, layout.total_codewords * 8);
    let mut modules: Vec<Vec<usize>> = lens.iter().map(|&k| vec![0; (k + e) * 8]).collect();
    let mut cw = 0;
    for i in 0..=short_len {
        for (b, &k) in lens.iter().enumerate() {
            if i < k {
                for j in 0..8 {
                    modules[b][i * 8 + j] = placed[cw * 8 + j];
                }
                cw += 1;
            }
        }
    }
    for i in 0..e {
        for (b, &k) in lens.iter().enumerate() {
            for j in 0..8 {
                modules[b][(k + i) * 8 + j] = placed[cw * 8 + j];
            }
            cw += 1;
        }
    }

    let generator = gf::generator(e);
    let blocks: Vec<Block> = modules
        .into_iter()
        .enumerate()
        .map(|(b, modules)| {
            let (k, start) = (lens[b], starts[b]);
            let block_data = &data[start..start + k];
            let mut bytes = block_data.to_vec();
            bytes.extend(gf::remainder(block_data, &generator));
            let len = (k + e) * 8;

            // One effect vector per free bit: the bit itself, and the ECC
            // of a lone bit there (RS is linear, so it scales).
            let mut flips: Vec<Bits> = Vec::new();
            let mut unit = vec![0u8; k];
            for p in 0..k {
                unit[p] = 1;
                let parity = gf::remainder(&unit[p..], &generator);
                unit[p] = 0;
                for j in 0..8 {
                    if (start + p) * 8 + j < free_from {
                        continue;
                    }
                    let c = 0x80u8 >> j;
                    let mut v = Bits::zeros(len);
                    v.set(p * 8 + j, true);
                    for (q, &r) in parity.iter().enumerate() {
                        let r = gf::mul(r, c);
                        for t in 0..8 {
                            if r >> (7 - t) & 1 != 0 {
                                v.set((k + q) * 8 + t, true);
                            }
                        }
                    }
                    flips.push(v);
                }
            }

            // Elimination in priority order: each wanted bit takes a free
            // vector that reaches it and is cleared from the vectors not yet
            // taken, so no later pivot touches it.
            let mut order: Vec<usize> = (0..len)
                .filter(|&i| wants[modules[i]].priority > 0)
                .collect();
            order.sort_by_key(|&i| (core::cmp::Reverse(wants[modules[i]].priority), modules[i]));
            let mut pivots = Vec::new();
            let mut next = 0;
            for at in order {
                if next == flips.len() {
                    break;
                }
                let Some(j) = (next..flips.len()).find(|&j| flips[j].get(at)) else {
                    continue;
                };
                flips.swap(next, j);
                let (pivot, rest) = flips[next..].split_first_mut().expect("next < len");
                for v in rest.iter_mut() {
                    if v.get(at) {
                        for (w, p) in v.words.iter_mut().zip(&pivot.words) {
                            *w ^= p;
                        }
                    }
                }
                pivots.push(at);
                next += 1;
            }
            flips.truncate(next);
            Block {
                base: Bits::from_bytes(&bytes),
                data_len: k,
                pivots: pivots.into_iter().zip(flips).collect(),
                modules,
            }
        })
        .collect();

    let total = |m: Mask| -> u64 {
        blocks
            .iter()
            .map(|b| score(b, &b.solve(m, wants, size), m, wants, size))
            .sum()
    };
    // Ties go to the lowest mask number.
    let chosen = mask.unwrap_or_else(|| {
        Mask::all()
            .max_by_key(|&m| (total(m), core::cmp::Reverse(m)))
            .expect("eight masks")
    });
    for (b, block) in blocks.iter().enumerate() {
        let bits = block.solve(chosen, wants, size);
        for i in 0..block.data_len {
            data[starts[b] + i] = bits.byte(i);
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matrix::{ModuleRole, QrMatrix};
    use crate::rng::SplitMix64;
    use crate::version::Version;
    use crate::{Ecc, QrCode};

    /// The codewords a matrix carries, unmasked, read back through the
    /// codeword map: `(interleaved codeword, block)` per module.
    fn codewords(qr: &QrMatrix) -> (Vec<u8>, Vec<u16>) {
        let size = qr.size() as usize;
        let layout = BlockLayout::new(qr.version(), qr.ecc());
        let mut words = vec![0u8; layout.total_codewords];
        let mut blocks = vec![0u16; layout.total_codewords];
        let map = qr.codeword_map();
        for &m in &matrix::bit_modules(qr.version(), layout.total_codewords * 8) {
            if let Some((cw, block)) = map[m] {
                let (x, y) = (m % size, m / size);
                let bit = qr.is_dark(x as u16, y as u16) ^ qr.mask().inverts(x, y);
                words[cw as usize] = words[cw as usize] << 1 | bit as u8;
                blocks[cw as usize] = block;
            }
        }
        (words, blocks)
    }

    fn random_wants(qr: &QrMatrix, seed: u64, share: f64) -> Vec<Want> {
        let mut rng = SplitMix64::new(seed);
        qr.modules()
            .map(|(_, _, _, role)| {
                let pick = !role.is_function() && rng.next_f64() < share;
                Want {
                    dark: rng.next_f64() < 0.5,
                    priority: if pick {
                        1 + (rng.next_u64() % 1000) as u32
                    } else {
                        0
                    },
                }
            })
            .collect()
    }

    fn steered(text: &str, version: u8, ecc: Ecc, wants: &[Want], mask: Option<Mask>) -> QrMatrix {
        let mut b = QrCode::builder()
            .ecc(ecc)
            .boost_ecc(false)
            .version(Version::new(version).unwrap());
        if let Some(m) = mask {
            b = b.mask(m);
        }
        let plans = b.plans_for_test(text);
        b.encode_steered_for_test(&plans, wants)
    }

    #[test]
    fn the_payload_and_reed_solomon_survive() {
        for (text, version, ecc) in [
            ("HELLO", 1, Ecc::L),
            ("https://example.com", 5, Ecc::M),
            ("https://example.com/menu", 10, Ecc::Q),
            ("x", 14, Ecc::H),
            ("freeqrent", 21, Ecc::L),
        ] {
            let plain = QrCode::builder()
                .ecc(ecc)
                .boost_ecc(false)
                .version(Version::new(version).unwrap())
                .encode_text(text)
                .unwrap();
            let wants = random_wants(&plain, version as u64, 0.7);
            let qr = steered(text, version, ecc, &wants, None);
            let layout = BlockLayout::new(qr.version(), qr.ecc());
            let (a, _) = codewords(&plain);
            let (b, blocks) = codewords(&qr);
            assert_ne!(a, b, "v{version}: something was steered");
            // De-interleave both and compare the payload bits.
            let blocks = &blocks;
            let data = |words: &[u8]| -> Vec<u8> {
                let mut out = Vec::new();
                for blk in 0..layout.num_blocks as u16 {
                    for i in 0..layout.data_codewords() {
                        if blocks[i] == blk {
                            out.push(words[i]);
                        }
                    }
                }
                out
            };
            let (da, db) = (data(&a), data(&b));
            let keep = qr.capacity_report().data_bits_used + 4;
            for bit in 0..keep.min(da.len() * 8) {
                let get = |d: &[u8]| d[bit / 8] >> (7 - bit % 8) & 1;
                assert_eq!(get(&da), get(&db), "v{version} bit {bit}");
            }
            let generator = gf::generator(layout.ecc_per_block);
            for blk in 0..layout.num_blocks as u16 {
                let of = |r: core::ops::Range<usize>| -> Vec<u8> {
                    r.filter(|&i| blocks[i] == blk).map(|i| b[i]).collect()
                };
                assert_eq!(
                    gf::remainder(&of(0..layout.data_codewords()), &generator),
                    of(layout.data_codewords()..layout.total_codewords),
                    "v{version} block {blk}"
                );
            }
        }
    }

    #[test]
    fn a_few_wants_per_block_are_always_met() {
        // Far fewer wants than free bits: every one is independent of the
        // others, so each must come out as asked.
        let plain = QrCode::builder()
            .ecc(Ecc::L)
            .boost_ecc(false)
            .version(Version::new(10).unwrap())
            .encode_text("hi")
            .unwrap();
        let wants = random_wants(&plain, 7, 0.02);
        let qr = steered("hi", 10, Ecc::L, &wants, None);
        let n = qr.size() as usize;
        for (i, w) in wants.iter().enumerate() {
            if w.priority > 0 {
                let (x, y) = ((i % n) as u16, (i / n) as u16);
                if qr.role(x, y) != ModuleRole::Remainder {
                    assert_eq!(qr.is_dark(x, y), w.dark, "({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn forced_masks_and_determinism() {
        let plain = QrCode::encode("https://example.com", Ecc::M).unwrap();
        let wants = random_wants(&plain, 3, 0.8);
        let version = plain.version().number();
        let mask = Mask::new(6);
        let a = steered("https://example.com", version, Ecc::M, &wants, mask);
        assert_eq!(Some(a.mask()), mask);
        let b = steered("https://example.com", version, Ecc::M, &wants, None);
        assert_eq!(
            b,
            steered("https://example.com", version, Ecc::M, &wants, None)
        );
    }
}
