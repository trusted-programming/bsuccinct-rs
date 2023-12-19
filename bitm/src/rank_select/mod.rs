mod utils;
mod select;
use self::select::{U64_PER_L1_ENTRY, U64_PER_L2_ENTRY, U64_PER_L2_RECORDS, Select0ForRank101111};
pub use self::select::{Select, Select0, BinaryRankSearch, CombinedSampling, SelectForRank101111};

use super::{ceiling_div, n_lowest_bits};
use dyn_size_of::GetSize;

/// Trait for rank operation that returns the number of ones (or zeros) in requested number of the first bits.
pub trait Rank {
    /// Returns the number of ones in first `index` bits or `None` if `index` is out of bound.
    fn try_rank(&self, index: usize) -> Option<usize>;

    /// Returns the number of ones in first `index` bits or panics if `index` is out of bound.
    #[inline] fn rank(&self, index: usize) -> usize {
        self.try_rank(index).expect("rank index out of bound")
    }

    /// Returns the number of ones in first `index` bits.
    /// The result is undefined if `index` is out of bound.
    #[inline] unsafe fn unchecked_rank(&self, index: usize) -> usize {
        self.rank(index)
    }

    /// Returns the number of zeros in first `index` bits or `None` if `index` is out of bound.
    #[inline] fn try_rank0(&self, index: usize) -> Option<usize> {
         self.try_rank(index).map(|r| index-r)
    }

    /// Returns the number of zeros in first `index` bits or panics if `index` is out of bound.
    #[inline] fn rank0(&self, index: usize) -> usize { index - self.rank(index) }

    /// Returns the number of ones in first `index` bits.
    /// The result is undefined if `index` is out of bound.
    #[inline] unsafe fn unchecked_rank0(&self, index: usize) -> usize {
        index - self.unchecked_rank(index)
    }
}

/// Returns number of bits set (to one) in `content`.
#[inline(always)] fn count_bits_in(content: &[u64]) -> usize {
    content.iter().map(|v| v.count_ones() as usize).sum()
}

/// The structure that holds array of bits `content` and `ranks` structure that takes no more than 3.125% extra space.
/// It can return the number of ones (or zeros) in first `index` bits of the `content` (see `rank` and `rank0` method) in *O(1)* time.
/// In addition, it supports select queries utilizing binary search over ranks (see [`BinaryRankSearch`])
/// or (optionally, at the cost of about 0.39% extra space overhead) combined sampling [`CombinedSampling`].
///
/// It uses modified version of the structure described in the paper:
/// - Zhou D., Andersen D.G., Kaminsky M. (2013) "Space-Efficient, High-Performance Rank and Select Structures on Uncompressed Bit Sequences".
///   In: Bonifaci V., Demetrescu C., Marchetti-Spaccamela A. (eds) Experimental Algorithms. SEA 2013.
///   Lecture Notes in Computer Science, vol 7933. Springer, Berlin, Heidelberg. <https://doi.org/10.1007/978-3-642-38527-8_15>
/// 
/// The modification consists of different level 2 entries that hold 4 rank values (r0 <= r1 <= r2 <= r3) relative to level 1 entry.
/// The content of level 2 entry, listing from the least significant bits, is:
/// - original: r0 stored on 32 bits, r1-r0 on 10 bits, r2-r1 on 10 bits, r3-r2 on 10 bits;
/// - our: r0 stored on 32 bits, r3-r0 on 11 bits, r2-r0 on 11 bits, r1-r0 on 10 bits
///        (and unused fields in the last entries, for out-of-bound content bits, are filled with bit ones).
/// With this layout, we can read the corresponding value in the rank operation without branching.
#[derive(Clone)]
pub struct ArrayWithRankSelect101111<Select = BinaryRankSearch, Select0 = BinaryRankSearch> {
    pub content: Box<[u64]>,  // BitVec
    pub l1ranks: Box<[usize]>,  // Each cell holds one rank using 64 bits
    pub l2ranks: Box<[u64]>,  // Each cell holds 4 ranks using [bits]: 32 (absolute), and, in reverse order (deltas): 10, 11, 11.
    select: Select,  // support for select (one)
    select0: Select0,  // support for select (zero)
}

impl<S: GetSize, S0: GetSize> GetSize for ArrayWithRankSelect101111<S, S0> {
    fn size_bytes_dyn(&self) -> usize {
        self.content.size_bytes_dyn() + self.l2ranks.size_bytes_dyn() + self.l1ranks.size_bytes_dyn() + self.select.size_bytes_dyn() + self.select0.size_bytes_dyn()
    }
    const USES_DYN_MEM: bool = true;
}

impl<S: SelectForRank101111, S0: Select0ForRank101111> From<Box<[u64]>> for ArrayWithRankSelect101111<S, S0> {
    #[inline] fn from(value: Box<[u64]>) -> Self { Self::build(value).0 }
}

impl<S: SelectForRank101111, S0> Select for ArrayWithRankSelect101111<S, S0> {
    fn try_select(&self, rank: usize) -> Option<usize> {
        self.select.select(&self.content, &self.l1ranks, &self.l2ranks, rank)
    }
}

impl<S, S0: Select0ForRank101111> Select0 for ArrayWithRankSelect101111<S, S0> {
    fn try_select0(&self, rank: usize) -> Option<usize> {
        self.select0.select0(&self.content, &self.l1ranks, &self.l2ranks, rank)
    }
}

impl<S: SelectForRank101111, S0: Select0ForRank101111> Rank for ArrayWithRankSelect101111<S, S0> {
    fn try_rank(&self, index: usize) -> Option<usize> {
        let block = index / 512;
        let word_idx = index / 64;
        // we start from access to content, as if given index of content is not out of bound,
        // then corresponding indices l1ranks and l2ranks are also not out of bound
        let mut r = (self.content.get(word_idx)? & n_lowest_bits(index as u8 % 64)).count_ones() as usize;
        let mut block_content = *unsafe{ self.l2ranks.get_unchecked(index/2048) };//self.ranks[block/4];
        r += unsafe{ *self.l1ranks.get_unchecked(index >> 32) } + (block_content & 0xFFFFFFFFu64) as usize; // 32 lowest bits   // for 34 bits: 0x3FFFFFFFFu64
        block_content >>= 32;   // remove the lowest 32 bits
        r += ((block_content >> (33 - 11 * (block & 3))) & 0b1_11111_11111) as usize;        
        Some(r + count_bits_in(unsafe {self.content.get_unchecked(block * 8..word_idx)}))
    }

    fn rank(&self, index: usize) -> usize {
        let block = index / 512;
        let mut block_content =  self.l2ranks[index/2048];//self.ranks[block/4];
        let mut r = unsafe{ *self.l1ranks.get_unchecked(index >> 32) } + (block_content & 0xFFFFFFFFu64) as usize; // 32 lowest bits   // for 34 bits: 0x3FFFFFFFFu64
        block_content >>= 32;   // remove the lowest 32 bits
        r += ((block_content >> (33 - 11 * (block & 3))) & 0b1_11111_11111) as usize;
        let word_idx = index / 64;
        r += count_bits_in(&self.content[block * 8..word_idx]);
        /*for w in block * (512 / 64)..word_idx {
            r += self.content[w].count_ones() as u64;
        }*/
        r + (self.content[word_idx] & n_lowest_bits(index as u8 % 64)).count_ones() as usize
    }
}

impl<S: SelectForRank101111, S0: Select0ForRank101111> ArrayWithRankSelect101111<S, S0> {
    pub fn build(content: Box<[u64]>) -> (Self, usize) {
        let mut l1ranks = Vec::with_capacity(ceiling_div(content.len(), U64_PER_L1_ENTRY));
        let mut l2ranks = Vec::with_capacity(ceiling_div(content.len(), U64_PER_L2_ENTRY));
        let mut current_total_rank: usize = 0;
        for content in content.chunks(U64_PER_L1_ENTRY) {  // each l1 chunk has 1<<32 bits = (1<<32)/64 content elements
            l1ranks.push(current_total_rank);
            let mut current_rank: u64 = 0;
            for chunk in content.chunks(U64_PER_L2_ENTRY) {   // each chunk has 32*64 = 2048 bits
                let mut to_append = current_rank;
                let mut vals = chunk.chunks(U64_PER_L2_RECORDS).map(|c| count_bits_in(c)); // each val has 8*64 = 512 bits
                if let Some(v) = vals.next() {
                    let mut chunk_sum = v as u64;  // now chunk_sum uses up to 10 bits
                    to_append |= chunk_sum << (32+11+11);
                    if let Some(v) = vals.next() {
                        chunk_sum += v as u64;     // now chunk_sum uses up to 11 bits
                        to_append |= chunk_sum << (32+11);
                        if let Some(v) = vals.next() {
                            chunk_sum += v as u64;     // now chunk_sum uses up to 11 bits
                            to_append |= chunk_sum << 32;
                            if let Some(v) = vals.next() { chunk_sum += v as u64; }
                        } else {
                            to_append |= ((1<<11)-1) << 32; // TODO powielic chunk_sum??
                        }
                    } else {
                        to_append |= ((1<<22)-1) << 32; // TODO powielic chunk_sum??
                    }
                    current_rank += chunk_sum;
                } else {
                    to_append |= 0xFF_FF_FF_FF << 32;   // TODO powielic chunk_sum??
                }
                l2ranks.push(to_append);
            }
            current_total_rank += current_rank as usize;
        }
        let l1ranks = l1ranks.into_boxed_slice();
        let l2ranks = l2ranks.into_boxed_slice();
        let select = S::new(&content, &l1ranks, &l2ranks, current_total_rank);
        let select0 = S0::new0(&content, &l1ranks, &l2ranks, current_total_rank);
        (Self{content, l1ranks, l2ranks, select, select0}, current_total_rank)
    }
}

impl<S: SelectForRank101111, S0: Select0ForRank101111> AsRef<[u64]> for ArrayWithRankSelect101111<S, S0> {
    #[inline] fn as_ref(&self) -> &[u64] { &self.content }
}


pub type ArrayWithRank101111 = ArrayWithRankSelect101111<BinaryRankSearch>;

/// The structure that holds array of bits `content` and `ranks` structure that takes no more than 6.25% extra space.
/// It can returns the number of ones in first `index` bits of the `content` (see `rank` method) in *O(1)* time.
#[derive(Clone)]
pub struct ArrayWithRankSimple {
    pub content: Box<[u64]>,  // BitVec
    pub ranks: Box<[u32]>,
}

impl GetSize for ArrayWithRankSimple {
    fn size_bytes_dyn(&self) -> usize {
        self.content.size_bytes_dyn() + self.ranks.size_bytes_dyn()
    }
    const USES_DYN_MEM: bool = true;
}

impl From<Box<[u64]>> for ArrayWithRankSimple {
    #[inline] fn from(value: Box<[u64]>) -> Self { Self::build(value).0 }
}

impl ArrayWithRankSimple {

    /// Constructs `ArrayWithRankSimple` and count number of bits set in `content`. Returns both.
    pub fn build(content: Box<[u64]>) -> (Self, u32) {
        let mut result = Vec::with_capacity(ceiling_div(content.len(), 8usize));
        let mut current_rank: u32 = 0;
        for seg_nr in 0..content.len() {
            if seg_nr % 8 == 0 { result.push(current_rank); }
            current_rank += content[seg_nr].count_ones();
        }
        (Self{content, ranks: result.into_boxed_slice()}, current_rank)
    }

    pub fn try_rank(&self, index: usize) -> Option<u32> {
        let word_idx = index / 64;
        let word_offset = index as u8 % 64;
        let block = index / 512;
        let mut r = (self.content.get(word_idx)? & n_lowest_bits(word_offset)).count_ones() as u32;
        r += unsafe{self.ranks.get_unchecked(block)};
        for w in block * (512 / 64)..word_idx {
            r += unsafe{self.content.get_unchecked(w)}.count_ones();
        }
        Some(r)
    }

    pub fn rank(&self, index: usize) -> u32 {
        let word_idx = index / 64;
        let word_offset = index as u8 % 64;
        let block = index / 512;
        let mut r = self.ranks[block];
        for w in block * (512 / 64)..word_idx {
            r += self.content[w].count_ones();
        }
        r + (self.content[word_idx] & n_lowest_bits(word_offset)).count_ones() as u32
    }

    //pub fn select(&self, rank: u32) -> usize {}
}

impl AsRef<[u64]> for ArrayWithRankSimple {
    #[inline] fn as_ref(&self) -> &[u64] { &self.content }
}

impl Rank for ArrayWithRankSimple {
    #[inline(always)] fn try_rank(&self, index: usize) -> Option<usize> {
        Self::try_rank(self, index).map(|r| r as usize)
    }

    #[inline(always)] fn rank(&self, index: usize) -> usize {
        Self::rank(self, index) as usize
    }
}

//impl Select for ArrayWithRankSimple {}

#[cfg(test)]
mod tests {
    use crate::BitAccess;
    use super::*;

    fn check_all_ones<ArrayWithRank: AsRef<[u64]> + Rank + Select>(a: &ArrayWithRank) {
        for (rank, index) in a.as_ref().bit_ones().enumerate() {
            assert_eq!(a.rank(index), rank, "rank({}) should be {}", index, rank);
            assert_eq!(a.select(rank), index, "select({}) should be {}", rank, index);
            //assert_eq!(a.try_rank(index), Some(rank), "rank({}) should be {}", index, rank);
            //assert_eq!(a.try_select(rank), Some(index), "select({}) should be {}", rank, index);
        }
    }

    fn check_all_zeros<ArrayWithRank: AsRef<[u64]> + Rank + Select0>(a: &ArrayWithRank) {
        for (rank, index) in a.as_ref().bit_zeros().enumerate() {
            assert_eq!(a.rank0(index), rank, "rank0({}) should be {}", index, rank);
            assert_eq!(a.select0(rank), index, "select0({}) should be {}", rank, index);
            //assert_eq!(a.try_rank0(index), Some(rank), "rank0({}) should be {}", index, rank);
            //assert_eq!(a.try_select0(rank), Some(index), "select0({}) should be {}", rank, index);
        }
    }

    fn test_empty_array_rank<ArrayWithRank: From<Box<[u64]>> + AsRef<[u64]> + Rank + Select + Select0>() {
        let a: ArrayWithRank = vec![].into_boxed_slice().into();
        assert_eq!(a.try_rank(0), None);
        assert_eq!(a.try_select(0), None);
    }

    #[test]
    fn test_empty_array_rank_101111() {
        test_empty_array_rank::<ArrayWithRank101111>();
    }

    #[test]
    fn test_empty_array_rank_101111_combined() {
        test_empty_array_rank::<ArrayWithRankSelect101111::<CombinedSampling, CombinedSampling>>();
    }

    fn test_array_with_rank<ArrayWithRank: From<Box<[u64]>> + AsRef<[u64]> + Rank + Select + Select0>() {
        let a: ArrayWithRank = vec![0b1101, 0b110].into_boxed_slice().into();
        assert_eq!(a.try_select(0), Some(0));
        assert_eq!(a.try_select(1), Some(2));
        assert_eq!(a.try_select(2), Some(3));
        assert_eq!(a.try_select(3), Some(65));
        assert_eq!(a.try_select(4), Some(66));
        assert_eq!(a.try_select(5), None);
        assert_eq!(a.rank(0), 0);
        assert_eq!(a.rank(1), 1);
        assert_eq!(a.rank(2), 1);
        assert_eq!(a.rank(3), 2);
        assert_eq!(a.rank(4), 3);
        assert_eq!(a.rank(8), 3);
        assert_eq!(a.rank(64), 3);
        assert_eq!(a.rank(65), 3);
        assert_eq!(a.rank(66), 4);
        assert_eq!(a.rank(67), 5);
        assert_eq!(a.rank(70), 5);
        assert_eq!(a.try_rank(127), Some(5));
        assert_eq!(a.try_rank(128), None);
        check_all_ones(&a);
        check_all_zeros(&a);
    }

    #[test]
    fn array_with_rank_101111() {
        test_array_with_rank::<ArrayWithRank101111>();
    }

    #[test]
    fn array_with_rank_101111_combined() {
        test_array_with_rank::<ArrayWithRankSelect101111::<CombinedSampling, CombinedSampling>>();
    }

    /*#[test]
    fn array_with_rank_simple() {
        test_array_with_rank::<ArrayWithRankSimple>();
    }*/

    fn test_big_array_with_rank<ArrayWithRank: From<Box<[u64]>> + AsRef<[u64]> + Rank + Select + Select0>() {
        let a: ArrayWithRank = vec![0b1101; 60].into_boxed_slice().into();
        assert_eq!(a.try_select(0), Some(0));
        assert_eq!(a.try_select(1), Some(2));
        assert_eq!(a.try_select(2), Some(3));
        assert_eq!(a.try_select(3), Some(64));
        assert_eq!(a.try_select(4), Some(66));
        assert_eq!(a.try_select(5), Some(67));
        assert_eq!(a.try_select(6), Some(128));
        assert_eq!(a.try_select(7), Some(130));
        assert_eq!(a.try_select(3*8), Some(512));
        assert_eq!(a.try_select(3*8+1), Some(514));
        assert_eq!(a.try_select(2*6*8), Some(2*1024));
        assert_eq!(a.try_select(2*6*8+1), Some(2*1024+2));
        assert_eq!(a.try_select(2*6*8+2), Some(2*1024+3));
        assert_eq!(a.try_select(60*3), None);
        assert_eq!(a.rank(0), 0);
        assert_eq!(a.rank(1), 1);
        assert_eq!(a.rank(2), 1);
        assert_eq!(a.rank(3), 2);
        assert_eq!(a.rank(4), 3);
        assert_eq!(a.rank(8), 3);
        assert_eq!(a.rank(64), 3);
        assert_eq!(a.rank(65), 4);
        assert_eq!(a.rank(66), 4);
        assert_eq!(a.rank(67), 5);
        assert_eq!(a.rank(68), 6);
        assert_eq!(a.rank(69), 6);
        assert_eq!(a.rank(128), 6);
        assert_eq!(a.rank(129), 7);
        assert_eq!(a.rank(512), 3*8);
        assert_eq!(a.rank(513), 3*8+1);
        assert_eq!(a.rank(514), 3*8+1);
        assert_eq!(a.rank(515), 3*8+2);
        assert_eq!(a.rank(1024), 6*8);
        assert_eq!(a.rank(2*1024), 2*6*8);
        assert_eq!(a.rank(2*1024+1), 2*6*8+1);
        assert_eq!(a.rank(2*1024+2), 2*6*8+1);
        assert_eq!(a.rank(2*1024+3), 2*6*8+2);
        assert_eq!(a.try_rank(60*64), None);
        check_all_ones(&a);
        check_all_zeros(&a);
    }

    #[test]
    fn big_array_with_rank_101111() {
        test_big_array_with_rank::<ArrayWithRank101111>();
    }

    #[test]
    fn big_array_with_rank_101111_combined() {
        test_big_array_with_rank::<ArrayWithRankSelect101111::<CombinedSampling, CombinedSampling>>();
    }

    /*#[test]
    fn big_array_with_rank_simple() {
        test_big_array_with_rank::<ArrayWithRankSimple>();
    }*/

    fn test_content<ArrayWithRank: From<Box<[u64]>> + AsRef<[u64]> + Rank + Select + Select0>() {
        let a: ArrayWithRank = vec![u64::MAX; 35].into_boxed_slice().into();
        check_all_ones(&a);
        check_all_zeros(&a);
    }

    #[test]
    fn content_101111() {
        test_content::<ArrayWithRank101111>();
    }

    #[test]
    fn content_101111_combined() {
        test_content::<ArrayWithRankSelect101111::<CombinedSampling, CombinedSampling>>();
    }

    /*#[test]
    fn content_simple() {
        test_content::<ArrayWithRankSimple>();
    }*/

    fn array_64bit<ArrayWithRank: From<Box<[u64]>> + AsRef<[u64]> + Rank + Select + Select0>() {
        const SEGMENTS: usize = (1<<32)/64 * 2;
        let a: ArrayWithRank = vec![0b01_01_01_01; SEGMENTS].into_boxed_slice().into();
        assert_eq!(a.try_select(268435456), Some(4294967296));
        assert_eq!(a.try_select(268435456+1), Some(4294967296+2));
        assert_eq!(a.try_select(268435456+2), Some(4294967296+4));
        assert_eq!(a.try_select(268435456+3), Some(4294967296+6));
        assert_eq!(a.try_select(0), Some(0));
        assert_eq!(a.try_select(1), Some(2));
        assert_eq!(a.rank(0), 0);
        assert_eq!(a.rank(1), 1);
        assert_eq!(a.rank(2), 1);
        assert_eq!(a.rank(1<<32), (1<<(32-6)) * 4);
        assert_eq!(a.rank((1<<32)+1), (1<<(32-6)) * 4 + 1);
        assert_eq!(a.rank((1<<32)+2), (1<<(32-6)) * 4 + 1);
        assert_eq!(a.rank((1<<32)+3), (1<<(32-6)) * 4 + 2);
        assert_eq!(a.try_rank(SEGMENTS*64), None);
        check_all_ones(&a);
        check_all_zeros(&a);
    }

    #[test]
    #[ignore = "uses much memory and time"]
    fn array_64bit_101111_binary() {
        array_64bit::<ArrayWithRank101111>();
    }

    #[test]
    #[ignore = "uses much memory and time"]
    fn array_64bit_101111_combined() {
        array_64bit::<ArrayWithRankSelect101111::<CombinedSampling, CombinedSampling>>();
    }

    fn array_64bit_filled<ArrayWithRank: From<Box<[u64]>> + AsRef<[u64]> + Rank + Select>() {
        const SEGMENTS: usize = (1<<32)/64 * 2;
        let a: ArrayWithRank = vec![u64::MAX; SEGMENTS].into_boxed_slice().into();
        assert_eq!(a.select(4294965248), 4294965248);
        assert_eq!(a.rank(0), 0);
        assert_eq!(a.rank(1), 1);
        assert_eq!(a.rank(2), 2);
        for i in (1<<32)..(1<<32)+2048 {
            assert_eq!(a.rank(i), i);
            assert_eq!(a.select(i), i);
        }
        //check_all_ones(a);
    }

    #[test]
    #[ignore = "uses much memory and time"]
    fn array_64bit_filled_101111() {
        array_64bit_filled::<ArrayWithRank101111>();
    }

    #[test]
    #[ignore = "uses much memory and time"]
    fn array_64bit_filled_101111_combined() {
        array_64bit_filled::<ArrayWithRankSelect101111::<CombinedSampling, CombinedSampling>>();
    }

    fn array_64bit_halffilled<ArrayWithRank: From<Box<[u64]>> + AsRef<[u64]> + Rank + Select + Select0>() {
        const SEGMENTS: usize = (1<<32)/64 * 2;
        let a: ArrayWithRank = vec![0x5555_5555_5555_5555; SEGMENTS].into_boxed_slice().into();
        check_all_ones(&a);
        check_all_zeros(&a);
    }

    #[test]
    #[ignore = "uses much memory and time"]
    fn array_64bit_halffilled_101111_binary() {
        array_64bit_halffilled::<ArrayWithRank101111>();
    }

    #[test]
    #[ignore = "uses much memory and time"]
    fn array_64bit_halffilled_101111_combined() {
        array_64bit_halffilled::<ArrayWithRankSelect101111::<CombinedSampling, CombinedSampling>>();
    }

    fn array_64bit_zeroed_first<ArrayWithRank: From<Box<[u64]>> + AsRef<[u64]> + Rank + Select>() {
        const SEGMENTS: usize = (1<<32)/64 + 1;
        let mut content = vec![0; SEGMENTS].into_boxed_slice();
        content[SEGMENTS-1] = 0b11<<62;
        let a: ArrayWithRank = content.into();
        assert_eq!(a.rank(0), 0);
        assert_eq!(a.rank((1<<32)-1), 0);
        assert_eq!(a.rank(1<<32), 0);
        assert_eq!(a.rank((1<<32)+62), 0);
        assert_eq!(a.rank((1<<32)+63), 1);
        assert_eq!(a.select(0), (1<<32)+62);
        assert_eq!(a.select(1), (1<<32)+63);
        assert_eq!(a.try_select(2), None);
    }

    #[test]
    #[ignore = "uses much memory and time"]
    fn array_64bit_zeroed_first_101111() {
        array_64bit_zeroed_first::<ArrayWithRank101111>();
    }

    #[test]
    #[ignore = "uses much memory and time"]
    fn array_64bit_zeroed_first_101111_combined() {
        array_64bit_zeroed_first::<ArrayWithRankSelect101111::<CombinedSampling, CombinedSampling>>();
    }
}