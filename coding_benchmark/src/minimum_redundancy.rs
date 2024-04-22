use std::collections::{HashMap, HashSet};
use std::hint::black_box;
use std::sync::Arc;
use std::thread;

use bitm::{BitAccess, BitVec};
use butils::UnitPrefix;
use dyn_size_of::GetSize;
use minimum_redundancy::Frequencies;
use minimum_redundancy::{BitsPerFragment, Code, Coding};

use crate::compare_texts;

/// Prints speed of and returns counting symbol occurrences.
pub fn frequencies_u8(conf: &super::Conf, text: &[u8]) -> [usize; 256] {
    if conf.extra_test {
        conf.print_speed(
            " Counting symbol occurrences with array (u8 specific method)",
            conf.measure(|| <[usize; 256]>::with_occurrences_of(text.iter())),
        );
    }
    let result = <[usize; 256]>::with_occurrences_of(text.iter());
    println!(
        " Input of length {} consists of {} different symbols, its entropy is {:.2} bits/symbol.",
        text.len(),
        result.number_of_occurring_values(),
        result.entropy()
    );
    result
}

/// Prints speed of and returns counting symbol occurrences.
pub fn frequencies(conf: &super::Conf, text: &[u8]) -> HashMap<u8, usize> {
    if conf.extra_test {
        conf.print_speed(
            " Counting symbol occurrences with HashMap (generic method)",
            conf.measure(|| HashMap::<u8, usize>::with_occurrences_of(text.iter())),
        );
    }
    let result = HashMap::<u8, usize>::with_occurrences_of(text.iter());
    println!(
        " Input of length {} consists of {} different symbols, its entropy is {:.2} bits/symbol.",
        text.len(),
        result.number_of_occurring_values(),
        result.entropy()
    );
    result
}

#[inline(always)]
fn total_size_bits_u8(frequencies: &[usize; 256], book: &[minimum_redundancy::Code; 256]) -> usize {
    frequencies.frequencies().fold(0usize, |acc, (k, w)| {
        acc + book[k as usize].len as usize * w
    })
}

#[inline(always)]
fn compress_u8<'i>(
    text: impl IntoIterator<Item = &'i u8>,
    book: &[minimum_redundancy::Code; 256],
    compressed_size_bits: usize,
) -> Box<[u64]> {
    let mut compressed_text = Box::<[u64]>::with_zeroed_bits(compressed_size_bits);
    let mut bit_index = 0usize;
    for k in text {
        let c = book[*k as usize];
        compressed_text.init_bits(bit_index, c.content as u64, c.len.min(32) as u8);
        bit_index += c.len as usize;
    }
    assert_eq!(bit_index, compressed_size_bits);
    compressed_text
}

#[inline(always)]
fn total_size_bits(frequencies: &HashMap<u8, usize>, book: &HashMap<u8, Code>) -> usize {
    frequencies
        .iter()
        .fold(0usize, |acc, (k, w)| acc + book[&k].len as usize * *w)
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
fn decode(coding: Arc<Coding<u8>>, bits: Vec<bool>) {
    let unique_code_lengths: HashSet<u32> = coding.code_lengths().values().cloned().collect();
    let mut sorted_code_lengths: Vec<u32> = Vec::from_iter(unique_code_lengths);
    sorted_code_lengths.sort_unstable(); // sort_unstable is often faster and appropriate here

    let mut cursor = 0;
    let num_cores = num_cpus::get();
    let top_num_cores_code_lengths =
        &sorted_code_lengths[..num_cores.min(sorted_code_lengths.len())];

    while cursor as usize <= bits.len() {
        let coding_arc = coding.clone();
        let bits_portion = bits.clone();

        thread::spawn(move || {
            let mut d = coding_arc.decoder();
            let bits_iter = bits_portion.iter().skip(cursor as usize); // Starting from 'length' index

            for b in bits_iter {
                let frag = if *b { 1 } else { 0 };
                cursor += 1;
                if let minimum_redundancy::DecodingResult::Value(v) = d.consume(frag) {
                    black_box(v);
                    break;
                }
            }
        });

        for &length in top_num_cores_code_lengths {
            let coding_arc_2 = coding.clone();

            if length as usize > bits.len() {
                continue; // Skip if length is outside the bits array range
            }

            let bits_portion = bits.clone();
            thread::spawn(move || {
                let mut d = coding_arc_2.decoder();
                let bits_iter = bits_portion.iter().skip(length as usize); // Starting from 'length' index

                for b in bits_iter {
                    let frag = if *b { 1 } else { 0 };
                    if let minimum_redundancy::DecodingResult::Value(v) = d.consume(frag) {
                        black_box(v);
                        d.reset();
                    }
                }
            });
        }
    }
}

#[inline(always)]
fn decode_from_queue(
    coding: Arc<Coding<u8>>,
    compressed_text: &Box<[u64]>,
    total_size_bits: usize,
) {
    decode(
        coding,
        compressed_text
            .bit_in_range_iter(0..total_size_bits)
            .collect(),
    );
}

#[inline(always)]
fn decode_from_stack(
    coding: Arc<Coding<u8>>,
    compressed_text: &Box<[u64]>,
    total_size_bits: usize,
) {
    decode(
        coding,
        compressed_text
            .bit_in_range_iter(0..total_size_bits)
            .rev()
            .collect(),
    );
}

#[inline(always)]
fn decoded(
    coding: Arc<Coding<u8>>,
    uncompressed_len: usize,
    mut bits: impl Iterator<Item = bool>,
) -> Vec<u8> {
    let mut decoded_text = Vec::with_capacity(uncompressed_len);
    let mut d = coding.decoder();
    while let Some(b) = bits.next() {
        if let minimum_redundancy::DecodingResult::Value(v) = d.consume(b as u32) {
            decoded_text.push(*v);
            d.reset();
        }
    }
    decoded_text
}

fn verify_queue(
    text: &[u8],
    compressed_text: Box<[u64]>,
    coding: &Arc<Coding<u8>>,
    total_size_bits: usize,
) {
    print!(" Verifying decoding from a queue... ");
    compare_texts(
        &text,
        &decoded(
            Arc::clone(coding),
            compressed_text.len(),
            compressed_text.bit_in_range_iter(0..total_size_bits),
        ),
    );
}

fn verify_stack(
    text: &[u8],
    compressed_text: Box<[u64]>,
    coding: Arc<Coding<u8>>,
    total_size_bits: usize,
) {
    print!(" Verifying decoding from a stack... ");
    compare_texts(
        &text,
        &decoded(
            coding,
            compressed_text.len(),
            compressed_text.bit_in_range_iter(0..total_size_bits).rev(),
        ),
    );
}

pub fn benchmark_u8(conf: &super::Conf) {
    //println!("Measuring the performance of u8-specific minimum_redundancy version:");
    println!("### minimum_redundancy with u8-specific optimizations ###");

    let text = conf.text();
    let frequencies = frequencies_u8(conf, &text);

    let dec_constr_ns = conf
        .measure(|| Coding::from_frequencies_cloned(BitsPerFragment(1), &frequencies))
        .as_nanos();
    let coding = Arc::new(Coding::from_frequencies_cloned(
        BitsPerFragment(1),
        &frequencies,
    ));
    let enc_constr_ns = conf.measure(|| coding.codes_for_values_array()).as_nanos();
    let rev_enc_constr_ns = conf
        .measure(|| coding.reversed_codes_for_values_array())
        .as_nanos();

    println!(" Decoder + suffix (prefix) encoder construction time [ns]: {:.0} + {:.0} ({:.0}) = {:.0} ({:.0})",
         dec_constr_ns, enc_constr_ns, rev_enc_constr_ns, dec_constr_ns+enc_constr_ns, dec_constr_ns+rev_enc_constr_ns);
    println!(" Decoder size: {} bytes", coding.size_bytes());

    println!(" Prefix order:");
    let book = coding.reversed_codes_for_values_array();
    conf.print_speed(
        "  encoding without adding to bit vector",
        conf.measure(|| {
            for k in text.iter() {
                black_box(book[*k as usize]);
            }
        }),
    );
    conf.print_speed(
        "  encoding + adding to bit vector",
        conf.measure(|| compress_u8(text.iter(), &book, total_size_bits_u8(&frequencies, &book))),
    );
    let compressed_size_bits = total_size_bits_u8(&frequencies, &book);
    let compressed_text = compress_u8(text.iter(), &book, compressed_size_bits);
    conf.print_compressed_size(compressed_size_bits);
    conf.print_speed(
        "  decoding from a queue (without storing)",
        conf.measure(|| decode_from_queue(coding.clone(), &compressed_text, compressed_size_bits)),
    );
    if conf.verify {
        verify_queue(&text, compressed_text, &coding, compressed_size_bits);
    } else {
        drop(compressed_text);
    }

    println!(" Suffix order:");
    let book = coding.codes_for_values_array();
    conf.print_speed(
        "  encoding without adding to bit vector",
        conf.measure(|| {
            for k in text.iter() {
                black_box(book[*k as usize]);
            }
        }),
    );
    conf.print_speed(
        "  encoding + adding to bit vector",
        conf.measure(|| {
            compress_u8(
                text.iter().rev(),
                &book,
                total_size_bits_u8(&frequencies, &book),
            )
        }),
    );
    let compressed_size_bits = total_size_bits_u8(&frequencies, &book);
    let compressed_text = compress_u8(text.iter().rev(), &book, compressed_size_bits);
    conf.print_compressed_size(compressed_size_bits);
    conf.print_speed(
        "  decoding from a stack (without storing)",
        conf.measure(|| decode_from_stack(coding.clone(), &compressed_text, compressed_size_bits)),
    );
    if conf.verify {
        verify_stack(&text, compressed_text, coding, compressed_size_bits);
    }
}

pub fn benchmark(conf: &super::Conf) {
    //println!("Measuring the performance of the generic minimum_redundancy version:");
    println!("### minimum_redundancy, generic version ###");

    let text = conf.text();
    let frequencies = frequencies(conf, &text);

    let dec_constr_ns = conf
        .measure(|| Coding::from_frequencies_cloned(BitsPerFragment(1), &frequencies))
        .as_nanos();
    let coding = Arc::new(Coding::from_frequencies_cloned(
        BitsPerFragment(1),
        &frequencies,
    ));
    let enc_constr_ns = conf.measure(|| coding.codes_for_values()).as_nanos();
    let rev_enc_constr_ns = conf
        .measure(|| coding.reversed_codes_for_values())
        .as_nanos();

    println!(" Decoder + suffix (prefix) encoder construction time [ns]: {:.0} + {:.0} ({:.0}) = {:.0} ({:.0})",
         dec_constr_ns, enc_constr_ns, rev_enc_constr_ns, dec_constr_ns+enc_constr_ns, dec_constr_ns+rev_enc_constr_ns);
    println!(" Decoder size: {} bytes", coding.size_bytes());
    println!(" Prefix order:");
    let book = coding.reversed_codes_for_values();
    conf.print_speed(
        "  encoding without adding to bit vector",
        conf.measure(|| {
            for k in text.iter() {
                black_box(book.get(k));
            }
        }),
    );
    conf.print_speed(
        "  encoding + adding to bit vector",
        conf.measure(|| compress(text.iter(), &book, total_size_bits(&frequencies, &book))),
    );
    let compressed_size_bits = total_size_bits(&frequencies, &book);
    let compressed_text = compress(text.iter(), &book, compressed_size_bits);
    conf.print_compressed_size(compressed_size_bits);
    conf.print_speed(
        "  decoding from a queue (without storing)",
        conf.measure(|| decode_from_queue(coding.clone(), &compressed_text, compressed_size_bits)),
    );
    if conf.verify {
        verify_queue(&text, compressed_text, &coding, compressed_size_bits);
    } else {
        drop(compressed_text);
    }

    println!(" Suffix order:");
    let book = coding.codes_for_values();
    conf.print_speed(
        "  encoding without adding to bit vector",
        conf.measure(|| {
            for k in text.iter() {
                black_box(book.get(k));
            }
        }),
    );
    conf.print_speed(
        "  encoding + adding to bit vector",
        conf.measure(|| {
            compress(
                text.iter().rev(),
                &book,
                total_size_bits(&frequencies, &book),
            )
        }),
    );
    let compressed_size_bits = total_size_bits(&frequencies, &book);
    let compressed_text = compress(text.iter().rev(), &book, compressed_size_bits);
    conf.print_compressed_size(compressed_size_bits);
    conf.print_speed(
        "  decoding from a stack (without storing)",
        conf.measure(|| decode_from_stack(coding.clone(), &compressed_text, compressed_size_bits)),
    );
    if conf.verify {
        verify_stack(&text, compressed_text, coding, compressed_size_bits);
    }
}
