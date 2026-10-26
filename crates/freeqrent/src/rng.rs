//! The seeded generator behind every decorative choice (SPEC §4.1):
//! SplitMix64, so output stays deterministic.

/// SplitMix64 (Steele, Lea & Flood 2014).
#[derive(Clone, Debug)]
pub(crate) struct SplitMix64(u64);

impl SplitMix64 {
    pub(crate) fn new(seed: u64) -> SplitMix64 {
        SplitMix64(seed)
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub(crate) fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_reference_sequence() {
        // From the reference implementation, seed 1234567.
        let mut r = SplitMix64::new(1_234_567);
        assert_eq!(r.next_u64(), 6_457_827_717_110_365_317);
        assert_eq!(r.next_u64(), 3_203_168_211_198_807_973);
        assert!((0.0..1.0).contains(&r.next_f64()));
    }
}
