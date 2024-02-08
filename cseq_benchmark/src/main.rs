#![doc = include_str!("../README.md")]

mod elias_fano;
mod bitm;
mod sucds;
mod succinct;
#[cfg(feature = "vers-vecs")] mod vers;

use std::{hint::black_box, num::{NonZeroU32, NonZeroU64}, time::Instant};

use butils::XorShift64;
use clap::{Parser, Subcommand};

//#[allow(non_camel_case_types)]
//#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
#[derive(Subcommand)]
pub enum Structure {
    /// Elias-Fano
    EliasFano,
    /// Uncompressed bit vector from bitm library
    BitmBV,
    /// Uncompressed bit vector from sucds library
    SucdsBV,
    /// Uncompressed bit vector from succinct library
    SuccinctJacobson,
    /// Uncompressed bit vector from succinct library
    SuccinctRank9,
    /// Uncompressed bit vector from vers
    #[cfg(feature = "vers-vecs")] Vers,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
/// Compact sequences benchmark.
pub struct Conf {
    /// Structure to test
    #[command(subcommand)]
    pub structure: Structure,

    /// The number of items to use
    #[arg(short = 'n', long, default_value_t = 500_000_000)]
    pub num: usize,

    /// Item universe.
    #[arg(short = 'u', long, default_value_t = 1_000_000_000)]
    pub universe: usize,

    /// Time (in seconds) of measuring and warming up the CPU cache before measuring
    #[arg(short='t', long, default_value_t = 5)]
    pub time: u16,

    /// Whether to check the validity of built sequence
    #[arg(long, default_value_t = false)]
    pub verify: bool,

    /// Seed for (XorShift64) random number generator
    #[arg(short='s', long, default_value_t = NonZeroU64::new(1234).unwrap())]
    pub seed: NonZeroU64,

    // Number of pre-generated queries
    #[arg(short='q', long, default_value_t = NonZeroU32::new(1_000_000).unwrap())]
    pub queries: NonZeroU32,
}

impl Conf {
    //pub const QUERIES: usize = 1_000_000;
    //pub const STEPS_NUM: usize = 194_933;   // prime
    //pub const STEPS_NUM: usize = 1_949_333;

    fn rand_gen(&self) -> XorShift64 { XorShift64(self.seed.get()) }

    fn rand_queries(&self, query_universe: usize) -> Box<[usize]> {
        self.rand_gen().take(self.queries.get() as usize).map(|v| v as usize % query_universe).collect()
    }

    #[inline(always)] fn measure<F>(&self, f: F) -> f64
     where F: Fn()
    {
        let mut iters = 1;
        if self.time > 0 {
            let time = Instant::now();
            loop {
                f();
                if time.elapsed().as_secs() > self.time as u64 { break; }
                iters += 1;
            }
        }
        let start_moment = Instant::now();
        for _ in 0..iters { f(); }
        return start_moment.elapsed().as_secs_f64() / iters as f64
    }

    #[inline(always)] fn queries_measure<R, F>(&self, queries: &[usize], f: F) -> f64
    where F: Fn(usize) -> R
    {
        self.measure(|| for i in queries { black_box(f(*i)); }) / queries.len() as f64
    }

    #[inline(always)] fn num_queries_measure<R, F>(&self, f: F) -> f64
    where F: Fn(usize) -> R
    {
        self.queries_measure(&self.rand_queries(self.num), f)
    }

    #[inline(always)] fn num_complement_queries_measure<R, F>(&self, f: F) -> f64
    where F: Fn(usize) -> R
    {
        self.queries_measure(&self.rand_queries(self.universe - self.num), f)
    }

    #[inline(always)] fn universe_queries_measure<R, F>(&self, f: F) -> f64
    where F: Fn(usize) -> R
    {
        self.queries_measure(&self.rand_queries(self.universe), f)
    }

    /*#[inline(always)] fn sampling_measure<R, F>(&self, steps: StepBy<Range<usize>>, f: F) -> f64
    where F: Fn(usize) -> R
    {
        self.measure(|| for i in steps.clone() { black_box(f(i)); }) / steps.len() as f64
    }

    #[inline(always)] fn num_sampling_measure<R, F>(&self, steps_num: usize, f: F) -> f64
    where F: Fn(usize) -> R
    {
        self.sampling_measure((0..self.num).step_by((self.num / steps_num).max(1)), f)
    }

    #[inline(always)] fn num_complement_sampling_measure<R, F>(&self, steps_num: usize, f: F) -> f64
    where F: Fn(usize) -> R
    {
        let complement = self.universe - self.num;
        self.sampling_measure((0..complement).step_by((complement / steps_num).max(1)), f)
    }

    #[inline(always)] fn universe_sampling_measure<R, F>(&self, steps_num: usize, f: F) -> f64
    where F: Fn(usize) -> R
    {
        self.sampling_measure((0..self.universe).step_by((self.universe / steps_num).max(1)), f)
    }*/
}

fn percent_of(overhead: usize, whole: usize) -> f64 { (overhead*100) as f64 / whole as f64 }

fn main() {
    let conf: Conf = Conf::parse();
    match conf.structure {
        Structure::EliasFano => elias_fano::benchmark(&conf),
        Structure::BitmBV => bitm::benchmark_rank_select(&conf),
        Structure::SucdsBV => sucds::benchmark_rank9_select(&conf),
        Structure::SuccinctJacobson => succinct::benchmark_jacobson(&conf),
        Structure::SuccinctRank9 => succinct::benchmark_rank9(&conf),
        #[cfg(feature = "vers-vecs")] Structure::Vers => vers::benchmark_rank_select(&conf)
    }
}