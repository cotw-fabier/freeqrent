use crate::tables;

/// A QR code version (symbol size), 1 to 40.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(u8);

impl Version {
    /// Version 1, 21×21 modules.
    pub const MIN: Version = Version(1);
    /// Version 40, 177×177 modules.
    pub const MAX: Version = Version(40);

    /// Returns `None` unless `1 <= number <= 40`.
    pub const fn new(number: u8) -> Option<Version> {
        if number >= 1 && number <= 40 {
            Some(Version(number))
        } else {
            None
        }
    }

    /// The version number, 1 to 40.
    pub const fn number(self) -> u8 {
        self.0
    }

    /// Modules per side, without the quiet zone: `4 * version + 17`.
    pub const fn size(self) -> u16 {
        self.0 as u16 * 4 + 17
    }

    /// Which character-count width band the version is in: 0 for 1–9,
    /// 1 for 10–26, 2 for 27–40.
    pub(crate) const fn count_band(self) -> usize {
        match self.0 {
            1..=9 => 0,
            10..=26 => 1,
            _ => 2,
        }
    }

    /// The first version of each count band.
    pub(crate) const BAND_STARTS: [Version; 3] = [Version(1), Version(10), Version(27)];

    /// Every version from 1 to 40.
    pub fn all() -> impl DoubleEndedIterator<Item = Version> + Clone {
        (1..=40).map(Version)
    }

    /// Centers of the alignment patterns along one axis. The patterns sit at
    /// every pair of these, except the three pairs that overlap a finder.
    pub fn alignment_positions(self) -> impl ExactSizeIterator<Item = u16> + Clone {
        let v = self.0 as u16;
        let count = if v == 1 { 0 } else { v / 7 + 2 };
        // Spacing between the inner patterns, always even (ISO 18004 Annex E).
        let step = if count < 2 {
            0
        } else {
            (v * 8 + count * 3 + 5) / (count * 4 - 4) * 2
        };
        let last = self.size() - 7;
        (0..count).map(move |i| {
            if i == 0 {
                6
            } else {
                last - (count - 1 - i) * step
            }
        })
    }

    /// Modules that carry codewords (data, ECC, and remainder bits).
    pub(crate) const fn raw_data_modules(self) -> usize {
        let v = self.0 as usize;
        let mut result = (16 * v + 128) * v + 64;
        if v >= 2 {
            let align = v / 7 + 2;
            result -= (25 * align - 10) * align - 55;
            if v >= 7 {
                // Two 6×3 version blocks.
                result -= 36;
            }
        }
        result
    }

    /// Total codewords (data + ECC).
    pub(crate) const fn total_codewords(self) -> usize {
        self.raw_data_modules() / 8
    }
}

/// Error correction level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ecc {
    /// Recovers about 7% of codewords.
    L,
    /// Recovers about 15% of codewords.
    M,
    /// Recovers about 25% of codewords.
    Q,
    /// Recovers about 30% of codewords.
    H,
}

impl Ecc {
    /// All levels, lowest first.
    pub const ALL: [Ecc; 4] = [Ecc::L, Ecc::M, Ecc::Q, Ecc::H];

    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    /// The two format-information bits for this level.
    pub(crate) const fn format_bits(self) -> u32 {
        match self {
            Ecc::L => 1,
            Ecc::M => 0,
            Ecc::Q => 3,
            Ecc::H => 2,
        }
    }

    /// The next level up, if any.
    pub(crate) const fn higher(self) -> Option<Ecc> {
        match self {
            Ecc::L => Some(Ecc::M),
            Ecc::M => Some(Ecc::Q),
            Ecc::Q => Some(Ecc::H),
            Ecc::H => None,
        }
    }
}

/// How dense a grid of alignment boxes an art-mode code carries (SPEC
/// §7.8): a version range, picked instead of a version.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TrackingGrid {
    /// A 3 × 3 grid: 6 alignment boxes, versions 7–13 (45–69 modules).
    /// The one that scans fast on phones (SPEC §7.8).
    Sparse,
    /// A 4 × 4 grid: 13 boxes, versions 14–20 (73–97 modules). The default.
    /// Phones scan it slowly at clarity 0 (SPEC §7.8).
    #[default]
    Standard,
    /// A 5 × 5 grid: 22 boxes, versions 21–27 (101–125 modules).
    Dense,
}

impl TrackingGrid {
    /// The smallest and largest version with this grid.
    pub const fn versions(self) -> (Version, Version) {
        match self {
            TrackingGrid::Sparse => (Version(7), Version(13)),
            TrackingGrid::Standard => (Version(14), Version(20)),
            TrackingGrid::Dense => (Version(21), Version(27)),
        }
    }
}

/// Block structure of one version at one ECC level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BlockLayout {
    pub ecc_per_block: usize,
    pub num_blocks: usize,
    pub total_codewords: usize,
}

impl BlockLayout {
    pub(crate) fn new(version: Version, ecc: Ecc) -> BlockLayout {
        let v = version.number() as usize;
        BlockLayout {
            ecc_per_block: tables::ECC_CODEWORDS_PER_BLOCK[ecc.index()][v] as usize,
            num_blocks: tables::NUM_ECC_BLOCKS[ecc.index()][v] as usize,
            total_codewords: version.total_codewords(),
        }
    }

    pub(crate) fn ecc_codewords(&self) -> usize {
        self.ecc_per_block * self.num_blocks
    }

    pub(crate) fn data_codewords(&self) -> usize {
        self.total_codewords - self.ecc_codewords()
    }

    /// Blocks that have one data codeword fewer than the rest. They come first.
    pub(crate) fn num_short_blocks(&self) -> usize {
        self.num_blocks - self.total_codewords % self.num_blocks
    }

    /// Data codewords in a short block.
    pub(crate) fn short_block_data_len(&self) -> usize {
        self.total_codewords / self.num_blocks - self.ecc_per_block
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn v(n: u8) -> Version {
        Version::new(n).unwrap()
    }

    #[test]
    fn version_bounds() {
        assert_eq!(Version::new(0), None);
        assert_eq!(Version::new(41), None);
        assert_eq!(v(1).size(), 21);
        assert_eq!(v(40).size(), 177);
        assert_eq!(Version::all().count(), 40);
    }

    #[test]
    fn alignment_positions_match_iso_table() {
        let cases: &[(u8, &[u16])] = &[
            (1, &[]),
            (2, &[6, 18]),
            (6, &[6, 34]),
            (7, &[6, 22, 38]),
            (14, &[6, 26, 46, 66]),
            (21, &[6, 28, 50, 72, 94]),
            (32, &[6, 34, 60, 86, 112, 138]),
            (36, &[6, 24, 50, 76, 102, 128, 154]),
            (40, &[6, 30, 58, 86, 114, 142, 170]),
        ];
        for &(n, expected) in cases {
            let got: Vec<u16> = v(n).alignment_positions().collect();
            assert_eq!(got, expected, "version {n}");
        }
    }

    #[test]
    fn data_capacity_matches_iso_table() {
        // Data codewords from ISO 18004 Table 7.
        let cases = [
            (1, Ecc::L, 19),
            (1, Ecc::M, 16),
            (1, Ecc::Q, 13),
            (1, Ecc::H, 9),
            (10, Ecc::M, 216),
            (20, Ecc::Q, 485),
            (20, Ecc::H, 385),
            (27, Ecc::H, 628),
            (40, Ecc::L, 2956),
            (40, Ecc::H, 1276),
        ];
        for (n, ecc, data) in cases {
            assert_eq!(
                BlockLayout::new(v(n), ecc).data_codewords(),
                data,
                "v{n} {ecc:?}"
            );
        }
    }

    #[test]
    fn every_layout_splits_into_consistent_blocks() {
        for version in Version::all() {
            for ecc in Ecc::ALL {
                let b = BlockLayout::new(version, ecc);
                let short = b.num_short_blocks();
                let long = b.num_blocks - short;
                let short_total = short * (b.short_block_data_len() + b.ecc_per_block);
                let long_total = long * (b.short_block_data_len() + 1 + b.ecc_per_block);
                assert_eq!(short_total + long_total, b.total_codewords);
                assert!(short >= 1);
            }
        }
    }
}
