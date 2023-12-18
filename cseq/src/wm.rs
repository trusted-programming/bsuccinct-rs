use bitm::{BitAccess, BitVec, ArrayWithRankSelect101111, CombinedSampling, Rank};
use dyn_size_of::GetSize;

/// Constructs bit vectors for the (current) level of velvet matrix.
/// Stores bits of `lower_bits` values from previous level in to vectors:
/// - `upper_bit` stores the most significant bits (msb; shown by `upper_bit_mask`) of the subsequent values,
///     in the order from previous level,
/// - `lower_bits` stores all (`bits_per_value`) less significant bits (shown by `upper_bit_mask-1`)
///     of the subsequent values, stable sorted by most significant bits (msb).
/// The following values show the bit indices to insert the next value:
/// - `upper_index` is index of `upper_bit`,
/// - `lower_zero_index` is index of `lower_bits` to insert next value with 0 msb,
/// - `lower_one_index` is index of `lower_bits` to insert next value with 1 msb,
struct LevelBuilder {
    upper_bit: Box<[u64]>,
    upper_index: usize,
    lower_bits: Box<[u64]>,
    lower_zero_index: usize,
    lower_one_index: usize,
    upper_bit_mask: u64,
    bits_per_value: u8
}

impl LevelBuilder {
    /// Construct level builder for given level `total_len` in bits, `number_of_zeros` among the most significant bits
    /// and index of most significant bit (`index_of_bit_to_extract`).
    fn new(number_of_zeros: usize, total_len: usize, index_of_bit_to_extract: u8) -> Self {
        Self {
            upper_bit: Box::with_zeroed_bits(total_len),
            upper_index: 0,
            lower_bits: Box::with_zeroed_bits(total_len * index_of_bit_to_extract as usize),
            lower_zero_index: 0,
            lower_one_index: number_of_zeros * index_of_bit_to_extract as usize,
            upper_bit_mask: 1<<index_of_bit_to_extract,
            bits_per_value: index_of_bit_to_extract
        }
    }

    /// Add subsequent `value` from previous level to `self`.
    fn push(&mut self, value: u64) {
        let is_one = value & self.upper_bit_mask != 0;
        self.upper_bit.init_successive_bit(&mut self.upper_index, is_one);
        self.lower_bits.init_successive_bits(
            if is_one { &mut self.lower_one_index } else { &mut self.lower_zero_index },
            value & (self.upper_bit_mask-1), self.bits_per_value);
    }
}

struct WaveletMatrixLevel {
    /// Bits.
    bits: ArrayWithRankSelect101111::<CombinedSampling, CombinedSampling>,

    /// Number of zero bits.
    number_of_zeros: usize
}

impl GetSize for WaveletMatrixLevel {
    fn size_bytes_dyn(&self) -> usize { self.bits.size_bytes_dyn() }
    const USES_DYN_MEM: bool = true;
}

impl WaveletMatrixLevel {
    fn new(level: Box::<[u64]>, number_of_zeros: usize) -> Self {
        //let (bits, number_of_ones) = ArrayWithRank::build(level);
        //Self { bits, zeros: level_len - number_of_ones }
        Self { bits: level.into(), number_of_zeros }
    }
}

/// WaveletMatrix stores a sequence of [`len`] [`bits_per_value`]-bit symbols.
pub struct WaveletMatrix {
    levels: Box<[WaveletMatrixLevel]>,
    len: usize
}

impl WaveletMatrix {

    /// Returns number of stored values.
    #[inline] pub fn len(&self) -> usize { self.len }

    /// Returns whether the sequence is empty.
    #[inline] pub fn is_empty(&self) -> bool { self.len == 0 }

    /// Returns number of bits per value.
    #[inline] pub fn bits_per_value(&self) -> u8 { self.levels.len() as u8 }

    pub fn from_fn<I, F>(content: F, content_len: usize, bits_per_value: u8) -> Self
        where I: IntoIterator<Item = u64>, F: Fn() -> I
    {
        assert!(bits_per_value > 0 && bits_per_value <= 64);
        let mut levels = Vec::with_capacity(bits_per_value as usize);
        if bits_per_value == 1 {
            let mut level = Box::with_zeroed_bits(content_len);
            for (i, e) in content().into_iter().enumerate() {
                level.init_bit(i, e != 0);
            }
            levels.push(WaveletMatrixLevel::new(level, content_len));
            return Self { levels: levels.into_boxed_slice(), len: content_len };
        }
        let mut number_of_zeros = [0; 64];
        for mut e in content() {
            e = !e;
            for b in 0..bits_per_value {
                number_of_zeros[b as usize] += (e & 1) as usize;
                e >>= 1;
            }
        }
        let mut current_bit = bits_per_value - 1;
        let mut rest = {
            let mut level = LevelBuilder::new(
                number_of_zeros[current_bit as usize], content_len, current_bit);
            for e in content() {
                level.push(e);
            }
            levels.push(WaveletMatrixLevel::new(level.upper_bit, number_of_zeros[current_bit as usize]));
            level.lower_bits
        };
        while current_bit >= 2 {
            let rest_bits_per_value = current_bit;
            current_bit -= 1;
            let mut level = LevelBuilder::new(
                number_of_zeros[current_bit as usize], content_len, current_bit);
            for index in (0..content_len*rest_bits_per_value as usize).step_by(rest_bits_per_value as usize) {
                level.push(rest.get_bits(index, rest_bits_per_value));
            }
            rest = level.lower_bits;
            levels.push(WaveletMatrixLevel::new(level.upper_bit, number_of_zeros[current_bit as usize]));
        }
        levels.push(WaveletMatrixLevel::new(rest, number_of_zeros[0]));
        Self { levels: levels.into_boxed_slice(), len: content_len }
    }

    pub fn from_bits(content: &[u64], content_len: usize, bits_per_value: u8) -> Self {
        Self::from_fn(
            || { (0..content_len).map(|index| content.get_fragment(index, bits_per_value)) },
             content_len, bits_per_value)
    }

    pub fn get(&self, mut index: usize) -> Option<u64> {
        if index >= self.len() { return None; }
        let mut result = 0;
        for level in self.levels.iter() {
            result <<= 1;
            if level.bits.content.get_bit(index) {
                result |= 1;
                index = level.bits.rank(index) + level.number_of_zeros;
            } else {
                index = level.bits.rank0(index);
            }
        }
        Some(result)
    }

    #[inline] pub fn get_or_panic(&self, index: usize) -> u64 {
        self.get(index).expect("WaveletMatrix::get index out of bound")
    }
}

impl GetSize for WaveletMatrix {
    fn size_bytes_dyn(&self) -> usize { self.levels.size_bytes_dyn() }
    const USES_DYN_MEM: bool = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty() {
        let wm = WaveletMatrix::from_bits(&[], 0, 2);
        assert_eq!(wm.len(), 0);
        assert_eq!(wm.bits_per_value(), 2);
        assert_eq!(wm.get(0), None);
    }

    #[test]
    fn test_2_levels() {
        let wm = WaveletMatrix::from_bits(&[0b01_01_10_11], 4, 2);
        assert_eq!(wm.len(), 4);
        assert_eq!(wm.bits_per_value(), 2);
        assert_eq!(wm.get(0), Some(0b11));
        assert_eq!(wm.get(1), Some(0b10));
        assert_eq!(wm.get(2), Some(0b01));
        assert_eq!(wm.get(3), Some(0b01));
        assert_eq!(wm.get(4), None);
    }

    #[test]
    fn test_3_levels() {
        let wm = WaveletMatrix::from_bits(&[0b000_110], 2, 3);
        assert_eq!(wm.len(), 2);
        assert_eq!(wm.bits_per_value(), 3);
        assert_eq!(wm.get(0), Some(0b110));
        assert_eq!(wm.get(1), Some(0b000));
        assert_eq!(wm.get(2), None);
    }

    #[test]
    fn test_4_levels() {
        let wm = WaveletMatrix::from_bits(&[0b1101_1010_0000_0001_1011], 5, 4);
        assert_eq!(wm.len(), 5);
        assert_eq!(wm.bits_per_value(), 4);
        assert_eq!(wm.get(0), Some(0b1011));
        assert_eq!(wm.get(1), Some(0b0001));
        assert_eq!(wm.get(2), Some(0b0000));
        assert_eq!(wm.get(3), Some(0b1010));
        assert_eq!(wm.get(4), Some(0b1101));
        assert_eq!(wm.get(5), None);
    }

}