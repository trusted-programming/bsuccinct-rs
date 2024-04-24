use std::marker::PhantomData;

use crate::{Coding, DecodingResult, TreeDegree};

/// Decoder that decodes a value for given code, consuming one codeword fragment
/// (and going one level down the huffman tree) at a time.
///
/// Time complexity of decoding the whole code is:
/// - pessimistic: *O(length of the longest code)*
/// - expected: *O(log(number of values, i.e. length of `coding.values`))*
/// - optimistic: *O(1)*
///
/// Memory complexity: *O(1)*
pub struct Decoder<'huff, ValueType, D> {
    /// shift+fragment is a current position (node number, counting from the left) at current level.
    shift: u32,
    /// Number of leafs at all previous levels.
    first_leaf_nr: u32,
    /// Number of the current level.
    level: u32,
    /// Current level size = number of: internal nodes + leaves.
    level_size: u32,
    _phantom: &'huff PhantomData<(ValueType, D)>, // Use PhantomData to handle the generic type D
}

impl<'huff, ValueType, D: TreeDegree> Decoder<'huff, ValueType, D> {
    /// Constructs decoder for given `coding`.
    pub fn new(coding_degree: u32) -> Self {
        Self {
            shift: 0,
            first_leaf_nr: 0,
            level: 0,
            level_size: coding_degree,
            _phantom: &PhantomData,
        }
    }

    /// Resets `self` to initial state and makes it ready to decode next value.
    pub fn reset(&mut self, coding_degree: u32) {
        self.shift = 0;
        self.first_leaf_nr = 0;
        self.level = 0;
        self.level_size = coding_degree;
    }

    /// Returns the number of fragments consumed since construction or last reset.
    #[inline(always)]
    pub fn consumed_fragments(&self) -> u32 {
        self.level
    }

    /// Consumes a `fragment` of the codeword and returns:
    /// - a value if the given `fragment` finishes the valid codeword;
    /// - an [`DecodingResult::Incomplete`] if the codeword is incomplete and the next fragment is needed;
    /// - or [`DecodingResult::Invalid`] if the codeword is invalid (possible only for bits per fragment > 1).
    ///
    /// Result is undefined if `fragment` exceeds `tree_degree`.
    pub fn consume(
        &mut self,
        coding: &'huff Coding<ValueType, D>,
        fragment: u32,
    ) -> DecodingResult<&'huff ValueType> {
        self.shift += fragment;
        let internal_nodes_count = coding.internal_nodes_count[self.level as usize];
        return if self.shift < internal_nodes_count {
            // internal node, go level down
            self.shift = coding.degree * self.shift;
            self.first_leaf_nr += self.level_size - internal_nodes_count; // increase by number of leafs at current level
            self.level_size = coding.degree * internal_nodes_count;
            self.level += 1;
            DecodingResult::Incomplete
        } else {
            // leaf, return value or Invalid
            coding
                .values
                .get((self.first_leaf_nr + self.shift - internal_nodes_count) as usize)
                .into()
            //self.coding.values.get((self.first_leaf_nr + self.level_size + self.shift) as usize).into()
        };
    }

    /// Consumes a `fragment` of the codeword and returns:
    /// - a value if the given `fragment` finishes the valid codeword;
    /// - an [`DecodingResult::Incomplete`] if the codeword is incomplete and the next fragment is needed;
    /// - or [`DecodingResult::Invalid`] if the codeword is invalid (possible only for `degree` greater than 2)
    ///     or `fragment` is not less than `degree`.
    #[inline(always)]
    pub fn consume_checked(
        &mut self,
        coding: &'huff Coding<ValueType, D>,
        fragment: u32,
    ) -> DecodingResult<&'huff ValueType> {
        if fragment < coding.degree.as_u32() {
            self.consume(coding, fragment)
        } else {
            DecodingResult::Invalid
        }
    }

    /// Tries to decode and return a single value from the `fragments` iterator,
    /// consuming as many fragments as needed.
    ///
    /// To decode the next value, self must be [reset](Self::reset) first
    /// (see also [Self::decode_next]).
    ///
    /// In case of failure, returns:
    /// - [`DecodingResult::Incomplete`] if the iterator exhausted before the value was decoded
    ///   ([`Self::consumed_fragments`] enables checking if the iterator yielded any fragment before exhausting).
    /// - [`DecodingResult::Invalid`] if obtained invalid codeword (possible only for `degree` greater than 2).
    #[inline]
    pub fn decode<F: Into<u32>, I: Iterator<Item = F>>(
        &mut self,
        coding: &'huff Coding<ValueType, D>,
        fragments: &mut I,
    ) -> DecodingResult<&'huff ValueType> {
        while let Some(fragment) = fragments.next() {
            match self.consume(coding, fragment.into()) {
                DecodingResult::Incomplete => {}
                result => {
                    return result;
                }
            }
        }
        DecodingResult::Incomplete
    }

    #[inline]
    pub fn decode_vec<F: Clone + Into<u32>>(
        &mut self,
        coding: &'huff Coding<ValueType, D>,
        fragments: &[F],
    ) -> DecodingResult<&'huff ValueType> {
        // Use an index to manually iterate over the vector
        let mut index = 0;
        while index < fragments.len() {
            let fragment = &fragments[index];
            // Convert the fragment into u32 and pass it to self.consume
            match self.consume(coding, fragment.clone().into()) {
                DecodingResult::Incomplete => {
                    // Just increment the index if the result is Incomplete
                    index += 1;
                    continue;
                }
                result => {
                    // Return any other result
                    return result;
                }
            }
        }

        // Return Incomplete if all fragments are processed and no other result is returned
        DecodingResult::Incomplete
    }

    /// Tries to decode and return a single value from the `fragments` iterator,
    /// consuming as many fragments as needed.
    ///
    /// If successful, it [resets](Self::reset) `self` to be ready to decode the next value
    /// (see also [Self::decode]).
    ///
    /// In case of failure, returns:
    /// - [`DecodingResult::Incomplete`] if the iterator exhausted before the value was decoded
    ///   ([`Self::consumed_fragments`] enables checking if the iterator yielded any fragment before exhausting).
    /// - [`DecodingResult::Invalid`] if obtained invalid codeword (possible only for `degree` greater than 2).
    #[inline]
    pub fn decode_next<F: Into<u32>, I: Iterator<Item = F>>(
        &mut self,
        coding: &'huff Coding<ValueType, D>,
        fragments: &mut I,
    ) -> DecodingResult<&'huff ValueType> {
        let result = self.decode(coding, fragments);
        if let DecodingResult::Value(_) = result {
            self.reset(coding.degree.as_u32());
        }
        result
    }
    /*pub fn decode_next<F: Into<u32>, I: Iterator<Item = F>>(&mut self, fragments: &mut I) -> DecodingResult<&'huff ValueType> {
        while let Some(fragment) = fragments.next() {
            match self.consume(fragment.into()) {
                DecodingResult::Incomplete => {},
                result @ DecodingResult::Value(_) => {
                    self.reset();
                    return result;
                },
                result @ DecodingResult::Invalid => { return result; }
            }
        }
        DecodingResult::Incomplete
    }*/
}
