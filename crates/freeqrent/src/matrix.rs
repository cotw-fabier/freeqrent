use alloc::vec;
use alloc::vec::Vec;

use crate::version::{BlockLayout, Ecc, Version};

/// One of the three finder patterns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Corner {
    /// Top-left finder.
    TopLeft,
    /// Top-right finder.
    TopRight,
    /// Bottom-left finder.
    BottomLeft,
}

/// The rings of a finder pattern, from the outside in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FinderRing {
    /// The dark 7×7 border.
    Outer,
    /// The light ring between border and eye.
    Gap,
    /// The dark 3×3 center.
    Eye,
}

/// What a module is for. The styling layer draws each role differently and
/// only ever sacrifices `Data`, `Ecc`, and `Remainder` modules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModuleRole {
    /// Part of a finder pattern.
    Finder {
        /// Which finder.
        corner: Corner,
        /// Which ring of it.
        ring: FinderRing,
    },
    /// The light border around a finder.
    Separator,
    /// The alternating timing row or column.
    Timing,
    /// Part of an alignment pattern.
    Alignment {
        /// The pattern's center module `(x, y)`.
        center: (u16, u16),
    },
    /// Format information (ECC level and mask), both copies.
    Format,
    /// Version information (versions 7 and up), both copies.
    Version,
    /// The single always-dark module next to the bottom-left finder.
    DarkModule,
    /// A bit of a data codeword.
    Data,
    /// A bit of an error correction codeword.
    Ecc,
    /// Leftover bits after the last codeword (always light before masking).
    Remainder,
}

impl ModuleRole {
    /// True for every role that's fixed by the symbol's structure rather
    /// than its content.
    pub const fn is_function(self) -> bool {
        !matches!(
            self,
            ModuleRole::Data | ModuleRole::Ecc | ModuleRole::Remainder
        )
    }
}

/// One of the eight data masks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Mask(u8);

impl Mask {
    /// Returns `None` unless `pattern < 8`.
    pub const fn new(pattern: u8) -> Option<Mask> {
        if pattern < 8 {
            Some(Mask(pattern))
        } else {
            None
        }
    }

    /// The mask pattern reference, 0 to 7.
    pub const fn pattern(self) -> u8 {
        self.0
    }

    /// All eight masks.
    pub fn all() -> impl Iterator<Item = Mask> + Clone {
        (0..8).map(Mask)
    }

    // Written exactly as ISO 18004 Table 10 states them.
    #[allow(clippy::manual_is_multiple_of)]
    fn inverts(self, x: usize, y: usize) -> bool {
        match self.0 {
            0 => (x + y) % 2 == 0,
            1 => y % 2 == 0,
            2 => x % 3 == 0,
            3 => (x + y) % 3 == 0,
            4 => (x / 3 + y / 2) % 2 == 0,
            5 => x * y % 2 + x * y % 3 == 0,
            6 => (x * y % 2 + x * y % 3) % 2 == 0,
            _ => ((x + y) % 2 + x * y % 3) % 2 == 0,
        }
    }
}

/// A finished QR symbol: dark/light modules plus the role of each one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QrMatrix {
    version: Version,
    ecc: Ecc,
    mask: Mask,
    size: u16,
    payload_bits: usize,
    modules: Vec<bool>,
    roles: Vec<ModuleRole>,
}

impl QrMatrix {
    /// The symbol's version.
    pub fn version(&self) -> Version {
        self.version
    }

    /// The symbol's ECC level.
    pub fn ecc(&self) -> Ecc {
        self.ecc
    }

    /// The mask that was applied.
    pub fn mask(&self) -> Mask {
        self.mask
    }

    /// Modules per side, without the quiet zone.
    pub fn size(&self) -> u16 {
        self.size
    }

    /// True if the module at column `x`, row `y` is dark. Panics when out of range.
    pub fn is_dark(&self, x: u16, y: u16) -> bool {
        self.modules[self.index(x, y)]
    }

    /// The role of the module at column `x`, row `y`. Panics when out of range.
    pub fn role(&self, x: u16, y: u16) -> ModuleRole {
        self.roles[self.index(x, y)]
    }

    /// Every module as `(x, y, dark, role)`, row by row.
    pub fn modules(&self) -> impl Iterator<Item = (u16, u16, bool, ModuleRole)> + '_ {
        let size = self.size as usize;
        self.modules
            .iter()
            .zip(&self.roles)
            .enumerate()
            .map(move |(i, (&dark, &role))| ((i % size) as u16, (i / size) as u16, dark, role))
    }

    fn index(&self, x: u16, y: u16) -> usize {
        assert!(
            x < self.size && y < self.size,
            "module ({x}, {y}) out of range"
        );
        y as usize * self.size as usize + x as usize
    }

    /// How much of the symbol the payload uses, and how much damage it survives.
    pub fn capacity_report(&self) -> CapacityReport {
        let layout = BlockLayout::new(self.version, self.ecc);
        let correctable_per_block =
            (layout.ecc_per_block - misdecode_protection(self.version, self.ecc)) / 2;
        CapacityReport {
            data_bits_used: self.payload_bits,
            data_bits_available: layout.data_codewords() * 8,
            data_codewords: layout.data_codewords(),
            ecc_codewords: layout.ecc_codewords(),
            ecc_blocks: layout.num_blocks,
            correctable_codewords: correctable_per_block * layout.num_blocks,
            total_codewords: layout.total_codewords,
        }
    }

    /// For every module (row-major), the interleaved codeword it carries a
    /// bit of and that codeword's Reed–Solomon block; `None` for function
    /// patterns and remainder bits. Damage budgets count against blocks.
    pub(crate) fn codeword_map(&self) -> Vec<Option<(u16, u16)>> {
        let size = self.size as usize;
        let layout = BlockLayout::new(self.version, self.ecc);
        let blocks = interleaved_blocks(&layout);
        let mut map = vec![None; size * size];
        let mut bit = 0;
        for_each_placement(size, |idx| {
            if matches!(self.roles[idx], ModuleRole::Data | ModuleRole::Ecc) {
                let codeword = bit / 8;
                map[idx] = Some((codeword as u16, blocks[codeword]));
                bit += 1;
            }
        });
        map
    }

    /// Builds the symbol from its final codeword sequence (data then ECC,
    /// already interleaved). Picks the lowest-penalty mask unless one is given.
    pub(crate) fn build(
        version: Version,
        ecc: Ecc,
        payload_bits: usize,
        codewords: &[u8],
        data_codewords: usize,
        mask: Option<Mask>,
    ) -> QrMatrix {
        let mut m = Builder::new(version);
        m.draw_function_patterns();
        m.place_codewords(codewords, data_codewords);

        let mask = mask.unwrap_or_else(|| {
            Mask::all()
                .min_by_key(|&candidate| {
                    let mut trial = m.clone();
                    trial.apply_mask(candidate);
                    trial.draw_format(ecc, candidate);
                    trial.penalty()
                })
                .expect("eight masks")
        });
        m.apply_mask(mask);
        m.draw_format(ecc, mask);

        QrMatrix {
            version,
            ecc,
            mask,
            size: m.size as u16,
            payload_bits,
            modules: m.modules,
            roles: m.roles,
        }
    }
}

/// Mutable construction state. Roles start as `Remainder` and are overwritten
/// as patterns and codewords are placed.
#[derive(Clone)]
struct Builder {
    version: Version,
    size: usize,
    modules: Vec<bool>,
    roles: Vec<ModuleRole>,
}

impl Builder {
    fn new(version: Version) -> Builder {
        let size = version.size() as usize;
        Builder {
            version,
            size,
            modules: vec![false; size * size],
            roles: vec![ModuleRole::Remainder; size * size],
        }
    }

    fn set(&mut self, x: usize, y: usize, dark: bool, role: ModuleRole) {
        let i = y * self.size + x;
        self.modules[i] = dark;
        self.roles[i] = role;
    }

    fn draw_function_patterns(&mut self) {
        let size = self.size;
        for i in 0..size {
            self.set(6, i, i % 2 == 0, ModuleRole::Timing);
            self.set(i, 6, i % 2 == 0, ModuleRole::Timing);
        }

        self.draw_finder(3, 3, Corner::TopLeft);
        self.draw_finder(size - 4, 3, Corner::TopRight);
        self.draw_finder(3, size - 4, Corner::BottomLeft);

        let positions: Vec<usize> = self
            .version
            .alignment_positions()
            .map(usize::from)
            .collect();
        let last = positions.len().saturating_sub(1);
        for (i, &cx) in positions.iter().enumerate() {
            for (j, &cy) in positions.iter().enumerate() {
                // The three corners that hold a finder.
                let on_finder = (i, j) == (0, 0) || (i, j) == (0, last) || (i, j) == (last, 0);
                if !on_finder {
                    self.draw_alignment(cx, cy);
                }
            }
        }

        // Reserve format areas with placeholder bits so placement skips them.
        self.draw_format(Ecc::L, Mask(0));
        self.draw_version();
    }

    fn draw_finder(&mut self, cx: usize, cy: usize, corner: Corner) {
        for dy in -4i32..=4 {
            for dx in -4i32..=4 {
                let (x, y) = (cx as i32 + dx, cy as i32 + dy);
                if x < 0 || y < 0 || x >= self.size as i32 || y >= self.size as i32 {
                    continue;
                }
                let role = match dx.abs().max(dy.abs()) {
                    0 | 1 => ModuleRole::Finder {
                        corner,
                        ring: FinderRing::Eye,
                    },
                    2 => ModuleRole::Finder {
                        corner,
                        ring: FinderRing::Gap,
                    },
                    3 => ModuleRole::Finder {
                        corner,
                        ring: FinderRing::Outer,
                    },
                    _ => ModuleRole::Separator,
                };
                let dark = matches!(
                    role,
                    ModuleRole::Finder {
                        ring: FinderRing::Eye | FinderRing::Outer,
                        ..
                    }
                );
                self.set(x as usize, y as usize, dark, role);
            }
        }
    }

    fn draw_alignment(&mut self, cx: usize, cy: usize) {
        let center = (cx as u16, cy as u16);
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                let dark = dx.abs().max(dy.abs()) != 1;
                let (x, y) = ((cx as i32 + dx) as usize, (cy as i32 + dy) as usize);
                self.set(x, y, dark, ModuleRole::Alignment { center });
            }
        }
    }

    fn draw_format(&mut self, ecc: Ecc, mask: Mask) {
        let bits = format_bits(ecc, mask);
        let bit = |i: u32| (bits >> i) & 1 != 0;
        let size = self.size;
        let f = ModuleRole::Format;

        // Around the top-left finder.
        for i in 0..=5 {
            self.set(8, i, bit(i as u32), f);
        }
        self.set(8, 7, bit(6), f);
        self.set(8, 8, bit(7), f);
        self.set(7, 8, bit(8), f);
        for i in 9..15 {
            self.set(14 - i, 8, bit(i as u32), f);
        }

        // Split between the other two finders.
        for i in 0..8 {
            self.set(size - 1 - i, 8, bit(i as u32), f);
        }
        for i in 8..15 {
            self.set(8, size - 15 + i, bit(i as u32), f);
        }
        self.set(8, size - 8, true, ModuleRole::DarkModule);
    }

    fn draw_version(&mut self) {
        let v = self.version.number() as u32;
        if v < 7 {
            return;
        }
        let mut rem = v;
        for _ in 0..12 {
            rem = (rem << 1) ^ ((rem >> 11) * 0x1F25);
        }
        let bits = v << 12 | rem;
        for i in 0..18 {
            let dark = (bits >> i) & 1 != 0;
            let a = self.size - 11 + i % 3;
            let b = i / 3;
            self.set(a, b, dark, ModuleRole::Version);
            self.set(b, a, dark, ModuleRole::Version);
        }
    }

    /// Fills the free modules in zigzag order (see [`for_each_placement`]).
    fn place_codewords(&mut self, codewords: &[u8], data_codewords: usize) {
        let total_bits = codewords.len() * 8;
        let data_bits = data_codewords * 8;
        let mut i = 0;
        let mut roles = core::mem::take(&mut self.roles);
        for_each_placement(self.size, |idx| {
            if roles[idx] != ModuleRole::Remainder || i >= total_bits {
                return;
            }
            self.modules[idx] = (codewords[i / 8] >> (7 - i % 8)) & 1 != 0;
            roles[idx] = if i < data_bits {
                ModuleRole::Data
            } else {
                ModuleRole::Ecc
            };
            i += 1;
        });
        self.roles = roles;
        debug_assert_eq!(i, total_bits);
    }

    fn apply_mask(&mut self, mask: Mask) {
        let size = self.size;
        for (idx, role) in self.roles.iter().enumerate() {
            if !role.is_function() && mask.inverts(idx % size, idx / size) {
                self.modules[idx] ^= true;
            }
        }
    }

    /// ISO 18004 7.8.3 penalty score. Lower is better.
    fn penalty(&self) -> u32 {
        let size = self.size;
        let dark = |x: usize, y: usize| self.modules[y * size + x];
        let mut score = 0;

        // Rules 1 and 3, on rows then columns, from run lengths. Each line
        // is framed by light runs as long as the symbol, standing in for the
        // quiet zone.
        let mut runs: Vec<(bool, usize)> = Vec::with_capacity(size + 2);
        for transpose in [false, true] {
            for a in 0..size {
                runs.clear();
                runs.push((false, size));
                for b in 0..size {
                    let c = if transpose { dark(a, b) } else { dark(b, a) };
                    match runs.last_mut() {
                        Some((color, len)) if *color == c => *len += 1,
                        _ => runs.push((c, 1)),
                    }
                }
                match runs.last_mut() {
                    Some((false, len)) => *len += size,
                    _ => runs.push((false, size)),
                }

                // Rule 1: runs of five or more, framing excluded.
                let last = runs.len() - 1;
                for (i, &(_, len)) in runs.iter().enumerate() {
                    let len = len - size * (usize::from(i == 0) + usize::from(i == last));
                    if len >= 5 {
                        score += 3 + (len - 5) as u32;
                    }
                }

                // Rule 3: dark:light:dark:light:dark runs in ratio 1:1:3:1:1
                // (any width n), with light of at least 4n on one side and n
                // on the other. Counted once per qualifying side.
                for w in runs.windows(7).filter(|w| !w[0].0) {
                    let n = w[1].1;
                    let core = w[2].1 == n && w[3].1 == 3 * n && w[4].1 == n && w[5].1 == n;
                    if core {
                        let (before, after) = (w[0].1, w[6].1);
                        score += 40 * u32::from(before >= 4 * n && after >= n);
                        score += 40 * u32::from(after >= 4 * n && before >= n);
                    }
                }
            }
        }

        // Rule 2: 2×2 blocks of one color.
        for y in 0..size - 1 {
            for x in 0..size - 1 {
                let c = dark(x, y);
                if c == dark(x + 1, y) && c == dark(x, y + 1) && c == dark(x + 1, y + 1) {
                    score += 3;
                }
            }
        }

        // Rule 4: dark ratio away from 50%, 10 points per 5% step.
        let total = (size * size) as u32;
        let dark_count = self.modules.iter().filter(|&&d| d).count() as u32;
        let k = (dark_count * 20).abs_diff(total * 10).div_ceil(total) - 1;
        score + k * 10
    }
}

/// Data-bit and damage budget of a symbol, from [`QrMatrix::capacity_report`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapacityReport {
    /// Bits taken by the segments (mode headers, counts, and data), before
    /// the terminator and padding.
    pub data_bits_used: usize,
    /// Data bits the symbol holds at its version and ECC level.
    pub data_bits_available: usize,
    /// Data codewords in the symbol.
    pub data_codewords: usize,
    /// Error correction codewords in the symbol.
    pub ecc_codewords: usize,
    /// Number of Reed–Solomon blocks.
    pub ecc_blocks: usize,
    /// Codewords that can be wrong or erased and still decode, assuming the
    /// damage is spread evenly across blocks.
    pub correctable_codewords: usize,
    /// Data plus ECC codewords.
    pub total_codewords: usize,
}

impl CapacityReport {
    /// `correctable_codewords / total_codewords`, in thousandths: the share
    /// of the symbol's codewords that can be lost. Logo and halftone budgets
    /// spend from this.
    pub fn damage_tolerance_permille(&self) -> u32 {
        (self.correctable_codewords * 1000 / self.total_codewords) as u32
    }
}

/// Zigzag placement order: two-column strips from the right, alternating
/// up and down, skipping the vertical timing column. Calls `f` with every
/// module index in order; callers skip function modules themselves.
fn for_each_placement(size: usize, mut f: impl FnMut(usize)) {
    let mut right = size - 1;
    loop {
        if right == 6 {
            right = 5;
        }
        let upward = (right + 1) & 2 == 0;
        for vert in 0..size {
            let y = if upward { size - 1 - vert } else { vert };
            for x in [right, right - 1] {
                f(y * size + x);
            }
        }
        if right < 2 {
            break;
        }
        right -= 2;
    }
}

/// The block of each interleaved codeword: data column by column (short
/// blocks drop out of the last column), then ECC column by column.
fn interleaved_blocks(layout: &BlockLayout) -> Vec<u16> {
    let short = layout.num_short_blocks();
    let short_len = layout.short_block_data_len();
    let mut blocks = Vec::with_capacity(layout.total_codewords);
    for i in 0..=short_len {
        for b in 0..layout.num_blocks {
            if i < short_len || b >= short {
                blocks.push(b as u16);
            }
        }
    }
    for _ in 0..layout.ecc_per_block {
        blocks.extend((0..layout.num_blocks).map(|b| b as u16));
    }
    blocks
}

/// Codewords reserved against misdecodes in the smallest symbols
/// (ISO 18004 Table 9, the `p` column).
pub(crate) fn misdecode_protection(version: Version, ecc: Ecc) -> usize {
    match (version.number(), ecc) {
        (1, Ecc::L) => 3,
        (1, Ecc::M) | (2, Ecc::L) => 2,
        (1, _) | (3, Ecc::L) => 1,
        _ => 0,
    }
}

/// The 15 format bits: ECC level and mask, BCH(15, 5), XOR 0x5412.
fn format_bits(ecc: Ecc, mask: Mask) -> u32 {
    let data = ecc.format_bits() << 3 | mask.0 as u32;
    let mut rem = data;
    for _ in 0..10 {
        rem = (rem << 1) ^ ((rem >> 9) * 0x537);
    }
    (data << 10 | rem) ^ 0x5412
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codeword_map_rebuilds_valid_reed_solomon_blocks() {
        // Read every codeword back out of the modules through the map, then
        // check each block's ECC against its data. This only holds if the
        // map's codeword and block assignments are exactly right.
        for (text, version, ecc) in [
            ("HELLO", 1, Ecc::M),
            ("https://example.com/menu", 5, Ecc::Q),
            ("freeqrent", 10, Ecc::H),
            ("x", 21, Ecc::L),
            ("block layout", 33, Ecc::Q),
        ] {
            let qr = crate::QrCode::builder()
                .ecc(ecc)
                .boost_ecc(false)
                .version(Version::new(version).unwrap())
                .encode_text(text)
                .unwrap();
            let layout = BlockLayout::new(qr.version(), qr.ecc());
            let size = qr.size() as usize;
            let mut codewords = vec![0u8; layout.total_codewords];
            let mut bits = vec![0u8; layout.total_codewords];
            let mut block_of = vec![u16::MAX; layout.total_codewords];
            let map = qr.codeword_map();
            // Bits arrive MSB first in placement order.
            for_each_placement(size, |idx| {
                if let Some((cw, block)) = map[idx] {
                    let dark = qr.modules[idx] ^ qr.mask.inverts(idx % size, idx / size);
                    let cw = cw as usize;
                    codewords[cw] = codewords[cw] << 1 | dark as u8;
                    bits[cw] += 1;
                    assert!(block_of[cw] == u16::MAX || block_of[cw] == block);
                    block_of[cw] = block;
                }
            });
            assert!(
                bits.iter().all(|&b| b == 8),
                "v{version}: every codeword has 8 modules"
            );
            let generator = crate::gf::generator(layout.ecc_per_block);
            let data_total = layout.data_codewords();
            for b in 0..layout.num_blocks as u16 {
                let of_block = |range: core::ops::Range<usize>| -> Vec<u8> {
                    range
                        .filter(|&i| block_of[i] == b)
                        .map(|i| codewords[i])
                        .collect()
                };
                let data = of_block(0..data_total);
                let ecc_words = of_block(data_total..layout.total_codewords);
                assert_eq!(ecc_words.len(), layout.ecc_per_block);
                assert_eq!(
                    crate::gf::remainder(&data, &generator),
                    ecc_words,
                    "v{version} block {b}"
                );
            }
        }
    }

    #[test]
    fn format_bits_match_iso_table() {
        // ISO 18004 Table C.1.
        assert_eq!(format_bits(Ecc::M, Mask(0)), 0b101010000010010);
        assert_eq!(format_bits(Ecc::L, Mask(0)), 0b111011111000100);
        assert_eq!(format_bits(Ecc::H, Mask(7)), 0b000100000111011);
        assert_eq!(format_bits(Ecc::Q, Mask(5)), 0b010000110000011);
    }

    #[test]
    fn function_pattern_counts() {
        for version in Version::all() {
            let mut b = Builder::new(version);
            b.draw_function_patterns();
            let free = b
                .roles
                .iter()
                .filter(|r| **r == ModuleRole::Remainder)
                .count();
            assert_eq!(free, version.raw_data_modules(), "v{}", version.number());
        }
    }

    #[test]
    fn finder_rings() {
        let mut b = Builder::new(Version::MIN);
        b.draw_function_patterns();
        let role = |x: usize, y: usize| b.roles[y * b.size + x];
        assert_eq!(
            role(3, 3),
            ModuleRole::Finder {
                corner: Corner::TopLeft,
                ring: FinderRing::Eye
            }
        );
        assert_eq!(
            role(1, 3),
            ModuleRole::Finder {
                corner: Corner::TopLeft,
                ring: FinderRing::Gap
            }
        );
        assert_eq!(
            role(0, 0),
            ModuleRole::Finder {
                corner: Corner::TopLeft,
                ring: FinderRing::Outer
            }
        );
        assert_eq!(role(7, 0), ModuleRole::Separator);
        assert_eq!(
            role(20, 0),
            ModuleRole::Finder {
                corner: Corner::TopRight,
                ring: FinderRing::Outer
            }
        );
        assert_eq!(role(8, 13), ModuleRole::DarkModule);
        assert_eq!(role(10, 6), ModuleRole::Timing);
    }
}
