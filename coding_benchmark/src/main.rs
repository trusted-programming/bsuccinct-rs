#![doc = include_str!("../README.md")]
mod constriction;
mod huffman_compress;
mod minimum_redundancy;

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::{hint::black_box, time::Instant};

use clap::{Parser, Subcommand};

use libflate::gzip::Encoder;
use rand::distributions::WeightedIndex;
use rand::prelude::*;
use rand_pcg::Pcg64Mcg;

//#[allow(non_camel_case_types)]
//#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
#[derive(Subcommand)]
pub enum Coding {
    /// Huffman coding implementation from minimum_redundancy (generic)
    #[clap(visible_alias = "mr")]
    MinimumRedundancy,
    /// Huffman coding implementation from minimum_redundancy with u8 specific improvements
    #[clap(visible_alias = "mr8")]
    MinimumRedundancyU8,
    /// Huffman coding implementation from huffman-compress
    #[clap(visible_alias = "hc")]
    HuffmanCompress,
    /// Huffman coding implementation from constriction
    Constriction,
    /// Tests all supported coders
    All,
} // see https://github.com/clap-rs/clap_derive/blob/master/examples/subcommand_aliases.rs

/*fn parse_spread(s: &str) -> Result<f64, String> {
    let result: f64 = s
        .parse()
        .map_err(|_| format!("`{s}` isn't a float number"))?;
    if result >= 0.0 { Ok(result)
    } else { Err(format!("spread must be non-negative")) }
}*/

#[derive(Parser)]
#[command(author, version, about, long_about = None, infer_subcommands=true)]
/// Coding benchmark.
pub struct Conf {
    /// Coder to test
    #[command(subcommand)]
    pub coding: Coding,

    /// Length of the test text
    #[arg(short = 'l', long, default_value_t = 1024*1024)]
    pub len: usize,

    /// Number of different symbols in the test text.
    #[arg(long, default_value_t = 256, value_parser = clap::value_parser!(u16).range(1..=256))]
    pub symbols: u16,

    /// The spread of the number of symbols (0 for all about equal).
    /// Each successive symbol occurs 1+SPREAD/1000 times more often than the previous one.
    #[arg(short = 'r', long, default_value_t = 100)]
    pub spread: u32,
    //#[arg(short = 'r', long, default_value_t = 5.0, value_parser = parse_spread)]
    //pub spread: f64,
    /// Time (in seconds) of measuring and warming up the CPU cache before measuring
    #[arg(short = 't', long, default_value_t = 5)]
    pub time: u16,

    /// Time (in seconds) of cooling (sleeping) before warming up and measuring
    #[arg(short = 'c', long, default_value_t = 0)]
    pub cooling_time: u16,

    /// Whether to check the validity
    #[arg(long, default_value_t = false)]
    pub verify: bool,

    /// Seed for random number generators
    #[arg(short = 's', long, default_value_t = 1234)]
    pub seed: u64,
    //pub seed: NonZeroU64,
    /// Whether to perform additional, non-essential measurements
    #[arg(short = 'e', long, default_value_t = false)]
    pub extra_test: bool,
}

impl Conf {
    //fn rand_gen(&self) -> XorShift64 { XorShift64(self.seed.get()) }

    /// Returns pseudo-random text for testing.
    fn text(&self) -> Box<[u8]> {
        if self.len <= self.symbols as usize {
            return (0u8..=(self.len - 1) as u8).collect();
        }

        //let r = self.range.get() as u64;
        //let weights: Vec<_> = XorShift64(self.seed).take(self.symbols.get() as usize).map(|v| (v % r) + 1).collect();
        let spread = 1.0 + self.spread as f64 * 0.001;
        let weights: Vec<_> = (1..=self.symbols as i32 + 1)
            .map(|v| spread.powi(v))
            .collect(); // alternative: (v as f64).powi(self.spread as i32)
        let dist = WeightedIndex::new(weights).unwrap();
        let rng = Pcg64Mcg::seed_from_u64(self.seed);

        (0u8..=(self.symbols - 1) as u8)
            .chain(
                dist.sample_iter(rng)
                    .map(|v| v as u8)
                    .take(self.len - self.symbols as usize),
            )
            .collect()
    }

    #[allow(unused)]
    /// Returns LZ77 compressed image for testing.
    fn compressed_image_text(&self) -> Vec<u8> {
        let test_image_path = Self::get_random_test_image("./data").unwrap();
        let image_data = Self::read_png_file(test_image_path).unwrap();

        let compressed_image = Self::lz_compress(&image_data).unwrap();

        compressed_image
    }

    fn lz_compress(data: &[u8]) -> io::Result<Vec<u8>> {
        let mut encoder = Encoder::new(Vec::new())?;
        encoder.write_all(data)?;
        let compressed_data = encoder.finish().into_result()?;

        Ok(compressed_data)
    }

    fn read_png_file(path: String) -> io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        let mut file = File::open(path)?;
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn get_random_test_image<P: AsRef<Path>>(path: P) -> Result<String, String> {
        let mut rng = thread_rng();
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    if entry.file_type().ok()?.is_file() {
                        Some(entry.path())
                    } else {
                        None
                    }
                })
                .collect::<Vec<PathBuf>>(),
            Err(_) => return Err("Failed to read directory".to_string()),
        };

        if entries.is_empty() {
            Err("No files found in directory".to_string())
        } else {
            match entries[rng.gen_range(0..entries.len())].to_str() {
                Some(path) => Ok(path.to_string()),
                None => Err("Failed to convert path to string".to_string()),
            }
        }
    }

    #[inline(always)]
    fn measure<R, F>(&self, mut f: F) -> f64
    where
        F: FnMut() -> R,
    {
        if self.cooling_time > 0 {
            std::thread::sleep(std::time::Duration::from_secs(self.cooling_time as u64));
        }
        let mut iters = 1usize;
        if self.time > 0 {
            let time = Instant::now();
            loop {
                black_box(f());
                if time.elapsed().as_secs() > self.time as u64 {
                    break;
                }
                iters += 1;
            }
        }
        let start_moment = Instant::now();
        for _ in 0..iters {
            black_box(f());
        }
        return start_moment.elapsed().as_secs_f64() / iters as f64;
    }

    fn print_speed(&self, label: &str, sec: f64) {
        /*print!("{}:   ", label);
        if self.len >= 512 * 1024 * 1024 {
            print!("{:.0} ms   ", sec.as_milis());
        } else if self.len >= 512 * 1024 {
            print!("{:.0} µs   ", sec.as_micros());
        } else {
            print!("{:.0} ns   ", sec.as_nanos());
        }*/
        let mb = self.len as f64 / (1024 * 1024) as f64;
        println!("{}: {:.0} mb/sec", label, mb / sec);
    }

    fn print_compressed_size(&self, compressed_size_bits: usize) {
        let cs_f64 = compressed_size_bits as f64;
        println!(
            "  {:.2}:1 compression ratio, {:.2} bits/symbol (without dictionary)",
            (8 * self.len) as f64 / cs_f64,
            cs_f64 / self.len as f64,
        );
    }
}

fn compare_texts(original: &[u8], decoded: &[u8]) {
    if original.len() == decoded.len() {
        for (i, (e, g)) in original.iter().zip(decoded).enumerate() {
            if e != g {
                println!(
                    "FAIL: decoded text at index {} has {}, while the original has {}",
                    i, g, e
                );
                return;
            }
        }
    } else {
        println!(
            "FAIL: decoded text has length {} different from original {}",
            decoded.len(),
            original.len()
        );
    }
    println!("DONE")
}

fn main() {
    let conf: Conf = Conf::parse();
    match conf.coding {
        Coding::MinimumRedundancy => minimum_redundancy::benchmark(&conf),
        Coding::MinimumRedundancyU8 => minimum_redundancy::benchmark_u8(&conf),
        Coding::HuffmanCompress => huffman_compress::benchmark(&conf),
        Coding::Constriction => constriction::benchmark(&conf),
        Coding::All => {
            minimum_redundancy::benchmark(&conf);
            minimum_redundancy::benchmark_u8(&conf);
            huffman_compress::benchmark(&conf);
            constriction::benchmark(&conf);
        }
    }
}
