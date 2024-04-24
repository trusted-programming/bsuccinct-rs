use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bitm::{BitAccess, BitVec};

use minimum_redundancy::Frequencies;
use minimum_redundancy::{BitsPerFragment, Code, Coding, TreeDegree};

// Input Data:
// Frequencies: A: 6, B: 1, C: 6, D: 2, E: 5
//
//
//
fn main() {
    // Construct coding with 1 bit per fragment for values 'a', 'b', 'c',
    let text = r#"
    Lorem ipsum dolor sit amet, consectetur adipiscing elit. Phasellus nec iaculis mauris.
    @#$%^^&*()_+[]{}|;':",./<>?`~
    1234567890

    Curabitur sollicitudin, nisl a finibus eleifend, nulla mi cursus mi, id pulvinar magna dolor eget velit.
    Proin eget magna sit amet justo sodales ullamcorper. Pellentesque et metus turpis.
    Quisque in sagittis nulla. Morbi massa magna, viverra in dolor eget, congue ullamcorper libero.

    -----=====+++++*****|||||||||////////\\\\\\\\\~~~~~~~
    Phasellus volutpat, velit eget interdum tristique, libero odio condimentum massa, sit amet commodo eros libero eget erat.
    Aliquam erat volutpat. Nunc dapibus tortor vel mi dapibus sollicitudin.
    Curabitur pretium, nisi ut volutpat mollis, leo risus interdum arcu, eget facilisis quam felis id mauris.
    Ut convallis, magna sed dapibus tincidunt, nulla lacus sollicitudin nisi, id commodo est risus non nibh.

    🌟✨🎉🔥💥🚀✈️🌍🌌🐉🦄🍀🎭
    Sed non neque elit. Sed euismod nisi porta lorem mollis aliquam. Aenean eu leo quam.
    Pellentesque ornare sem lacinia quam venenatis vestibulum.

    多种语言文本:
    - 中文: 今天天气真好，阳光明媚。我们一起去公园散步如何？那里的花开得正盛，非常适合拍照留念。
    - 日本語: こんにちは、世界！今日はとてもいい天気ですね。一緒にお散歩に行きませんか？公園は今、花が綺麗に咲いていますよ。
    - русский: Привет мир! Сегодня прекрасная погода, идеальная для прогулок по парку. Пойдемте вместе насладиться природой.
    - العربية: مرحبا بالعالم. الطقس اليوم جميل جداً، مثالي لنزهة في الحديقة. هيا بنا نستمتع بجمال الطبيعة والزهور.
    - 한국어: 안녕하세요, 세계! 오늘은 날씨가 참 좋아요. 공원에서 산책 어때요? 꽃들이 활짝 피었답니다.
    - हिंदी: नमस्ते दुनिया! आज मौसम बहुत सुहावना है, पार्क में टहलने चलें? वहाँ के फूल बहुत खिले हुए हैं।

    Special Characters Test Line: !@#$%^&*()_+-=[]{}|;':",./<>?`~👽🌵🏰📘🚒
    "#;
    let text_u8 = text.as_bytes();
    let frequencies = frequencies(&text_u8);
    let coding = Arc::new(Coding::from_frequencies_cloned(
        BitsPerFragment(1),
        &frequencies,
    ));

    // use this to get the lenghts
    // collect shortest n lenghts to test on first guess
    // n is number of available threads
    let unique_code_lengths: HashSet<u32> = coding.code_lengths().values().cloned().collect();

    // Convert HashSet to a Vec and sort it
    let mut sorted_code_lengths: Vec<u32> = Vec::from_iter(unique_code_lengths);
    sorted_code_lengths.sort_unstable(); // sort_unstable is often faster and appropriate here
                                         // let num_cores = num_cpus::get();
                                         // let top_num_cores_code_lengths =
                                         //     &sorted_code_lengths[..num_cores.min(sorted_code_lengths.len())];

    let book = coding.reversed_codes_for_values();
    let compressed_size_bits = total_size_bits(&frequencies, &book);
    let compressed_text = compress(text_u8.iter(), &book, compressed_size_bits);

    let bits: Vec<bool> = compressed_text
        .bit_in_range_iter(0..compressed_size_bits)
        .collect();
    let text_bytes = decoded(coding, compressed_text.len(), bits);

    let original_text = std::str::from_utf8(&text_bytes).unwrap();
    assert_eq!(text, original_text);
    println!("{}", text == original_text);
}

#[inline(always)]
fn total_size_bits(frequencies: &HashMap<u8, usize>, book: &HashMap<u8, Code>) -> usize {
    frequencies
        .iter()
        .fold(0usize, |acc, (k, w)| acc + book[&k].len as usize * *w)
}

pub fn frequencies(text: &[u8]) -> HashMap<u8, usize> {
    HashMap::<u8, usize>::with_occurrences_of(text.iter())
}
#[inline(always)]
fn compress<'i>(
    text: impl IntoIterator<Item = &'i u8>,
    book: &HashMap<u8, Code>,
    compressed_size_bits: usize,
) -> Box<[u64]> {
    let mut compressed_text = Box::<[u64]>::with_zeroed_bits(compressed_size_bits);
    let mut bit_index = 0usize;
    for k in text {
        let c = book[k];
        compressed_text.init_bits(bit_index, c.content as u64, c.len.min(32) as u8);
        bit_index += c.len as usize;
    }
    assert_eq!(bit_index, compressed_size_bits);
    compressed_text
}

#[inline(always)]
fn decoded(coding: Arc<Coding<u8>>, uncompressed_len: usize, bits: Vec<bool>) -> Vec<u8> {
    let coding = coding.clone();
    let mut main_iter = bits.into_iter();

    let mut decoded_text = Vec::with_capacity(uncompressed_len);
    let mut d = coding.decoder();

    while let Some(b) = main_iter.next() {
        if let minimum_redundancy::DecodingResult::Value(v) = d.consume(&coding, b as u32) {
            decoded_text.push(*v);
            d.reset(coding.degree.as_u32());
        }
    }

    decoded_text
}
