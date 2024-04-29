Succinct data structures and other Rust libraries and programs by Piotr Beling.

[![Build Status](https://img.shields.io/github/actions/workflow/status/beling/bsuccinct-rs/rust.yml?style=flat-square)](https://github.com/beling/bsuccinct-rs/actions/)
[![](https://tokei.rs/b1/github/beling/bsuccinct-rs?type=Rust,Python&style=flat-square)](https://github.com/beling/bsuccinct-rs)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue?style=flat-square)](LICENSE-APACHE)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE-MIT)

Included libraries:
- `minimum_redundancy` ([crate](https://crates.io/crates/minimum_redundancy), [doc](https://docs.rs/minimum_redundancy)) - encode and decode data with binary or non-binary Huffman coding;

Included programs:
- `coding_benchmark` ([crate](https://crates.io/crates/coding_benchmark), [doc](https://docs.rs/coding_benchmark)) - benchmarking Huffman coding crates.

Everything is dual-licensed under [Apache 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT).

# Bibliography
When using `bsuccinct` for research purposes, please cite the following paper:
- Piotr Beling, *BSuccinct: Rust libraries and programs focused on succinct data structures*, SoftwareX, Volume 26, 2024, 101681, ISSN 2352-7110,
<https://doi.org/10.1016/j.softx.2024.101681>

# Installation
Programs can be compiled and installed from sources. To do this, a Rust compiler is needed.
The easiest way to obtain the compiler along with other necessary tools (like `cargo`) is
to use [rustup](https://www.rust-lang.org/tools/install).

Please follow the instructions at https://www.rust-lang.org/tools/install.

## Installing rust programs
Once Rust is installed, to compile and install a program from sources and with native optimizations, just execute:

```RUSTFLAGS="-C target-cpu=native" cargo install <program_name>```

for example

```RUSTFLAGS="-C target-cpu=native" cargo install coding_benchmark```
