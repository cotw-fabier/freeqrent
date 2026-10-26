use alloc::vec::Vec;

/// An append-only bit sequence, most significant bit first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BitBuffer {
    bits: Vec<bool>,
}

impl BitBuffer {
    pub(crate) fn new() -> BitBuffer {
        BitBuffer::default()
    }

    pub(crate) fn len(&self) -> usize {
        self.bits.len()
    }

    /// Appends the low `count` bits of `value`, high bit first.
    pub(crate) fn push(&mut self, value: u32, count: u8) {
        debug_assert!(count <= 31 && value >> count == 0);
        for i in (0..count).rev() {
            self.bits.push((value >> i) & 1 != 0);
        }
    }

    pub(crate) fn extend(&mut self, other: &BitBuffer) {
        self.bits.extend_from_slice(&other.bits);
    }

    /// Packs into bytes. The length must be a multiple of 8.
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        debug_assert_eq!(self.bits.len() % 8, 0);
        self.bits
            .chunks(8)
            .map(|chunk| chunk.iter().fold(0u8, |acc, &b| (acc << 1) | b as u8))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_msb_first() {
        let mut b = BitBuffer::new();
        b.push(0b0100, 4);
        b.push(0b1010_0101, 8);
        b.push(0b1111, 4);
        assert_eq!(b.len(), 16);
        assert_eq!(b.to_bytes(), [0b0100_1010, 0b0101_1111]);
    }
}
