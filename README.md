# RustGuard

**A Memory-Safe, `no_std` ASCON-128 Authenticated Encryption Library for IoT Packet Security**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![no_std](https://img.shields.io/badge/no__std-✓-lightgrey.svg)](https://docs.rust-embedded.org/book/intro/no-std.html)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)
[![Tests](https://img.shields.io/badge/tests-24%20passed-brightgreen.svg)](#testing)

---

> **Research Paper:** *"RustGuard: Design and Software Verification of a Memory-Safe, no_std ASCON-128 Authenticated Encryption Library for IoT Packet Security"*
> Taha Hunaid Ali, Farhan Khan — Habib University, Karachi, Pakistan
> [[PDF]](paper/RustGuard_IEEE_Paper.pdf) · [[LaTeX source]](paper/RustGuard_IEEE_Paper.tex)

---

## Overview

RustGuard provides a production-quality, fully verified Rust implementation of **ASCON-128**
— the algorithm selected by NIST in February 2023 as the lightweight cryptography standard
(NIST IR 8454) — together with a compact **IoT Packet Authentication Protocol (RustGuard-PAP)**.

ASCON-128's bitsliced, branchless S-box and zero-table-lookup design make it ideal for
resource-constrained IoT microcontrollers. This library brings those properties to Rust with
compile-time memory-safety guarantees that C implementations cannot provide.

### Key Properties

| Property | Value |
|---|---|
| Algorithm | ASCON-128 AEAD + ASCON-HASH (NIST IR 8454) |
| Key size | 128 bits |
| Tag size | 128 bits (16 bytes) |
| Nonce size | 128 bits (16 bytes) |
| `unsafe` code | **Zero** (`#![forbid(unsafe_code)]`) |
| `std` dependency | **None** (`#![no_std]`) |
| Heap allocation | **None** (`heapless` stack buffers only) |
| Test suite | **24/24 tests pass** |
| Text segment | **11,605 bytes** (x86-64 release, LTO) |

---

## Repository Structure

```
rustguard/
├── rustguard-core/          # ASCON-128 AEAD + ASCON-HASH implementation
│   ├── src/
│   │   ├── lib.rs           # Permutation, AEAD encrypt/decrypt, ASCON-HASH
│   │   └── tests.rs         # 15 unit tests (correctness + security)
│   └── Cargo.toml
│
├── rustguard-pap/           # IoT Packet Authentication Protocol
│   ├── src/lib.rs           # PacketBuilder, unwrap_packet, PAP wire format
│   ├── tests/integration.rs # 9 integration tests
│   └── Cargo.toml
│
├── rustguard-hal/           # [Future] Bare-metal STM32L476 firmware skeleton
│   ├── src/main.rs
│   ├── memory.x
│   └── .cargo/config.toml
│
├── paper/
│   ├── RustGuard_IEEE_Paper.pdf   # Compiled paper (ready to submit)
│   ├── RustGuard_IEEE_Paper.tex   # LaTeX source
│   └── IEEEtran.cls               # IEEE conference class
│
├── results/
│   ├── figures/             # All 7 publication-quality figures (180 dpi PNG)
│   │   ├── fig1_latency.png
│   │   ├── fig2_throughput.png
│   │   ├── fig3_permutation.png
│   │   ├── fig4_codesize.png
│   │   ├── fig5_overhead.png
│   │   ├── fig6_tests.png
│   │   └── fig7_breakdown.png
│   └── raw/
│       └── benchmark_x86_64.txt  # Raw benchmark output (N=10,000)
│
├── scripts/
│   ├── generate_figures.py  # Reproduce all figures from raw data
│   ├── run_benchmarks.sh    # Run the benchmark suite
│   └── push_to_github.sh    # One-command GitHub setup
│
├── Cargo.toml               # Workspace definition
├── .gitignore
├── LICENSE
└── README.md
```

---

## Getting Started

### Prerequisites

- Rust 1.75+ (`rustup update stable`)
- For embedded target: `rustup target add thumbv7em-none-eabihf`
- For figure generation: Python 3.10+, matplotlib, numpy

### Build

```bash
git clone https://github.com/ta08451/rustguard.git
cd rustguard
cargo build --release
```

### Test

```bash
cargo test
```

Expected output:
```
running 15 tests
test tests::hash_tests::hash_deterministic ... ok
test tests::hash_tests::hash_different_inputs ... ok
test tests::hash_tests::hash_empty_nonzero ... ok
test tests::hash_tests::hash_multi_block ... ok
test tests::nist_kat::determinism ... ok
test tests::nist_kat::kat_01_empty_pt_empty_ad ... ok
test tests::nist_kat::kat_02_one_byte_roundtrip ... ok
test tests::nist_kat::kat_03_one_full_block ... ok
test tests::nist_kat::kat_04_two_blocks_with_ad ... ok
test tests::nist_kat::kat_05_partial_block_with_ad ... ok
test tests::nist_kat::sec_nonce_uniqueness ... ok
test tests::nist_kat::sec_tamper_ad_fails ... ok
test tests::nist_kat::sec_tamper_ciphertext_fails_and_zeroizes ... ok
test tests::nist_kat::sec_tamper_tag_fails ... ok
test tests::nist_kat::sec_wrong_key_fails ... ok
test result: ok. 15 passed; 0 failed

running 9 tests
test test_build_and_unwrap_32byte_payload ... ok
test test_minimum_packet_size_check ... ok
test test_replay_detection ... ok
test test_sequence_counter_increments ... ok
test test_tamper_ciphertext_rejected ... ok
test test_tamper_header_rejected ... ok
test test_tamper_tag_rejected ... ok
test test_variable_payload_sizes ... ok
test test_wrong_key_fails ... ok
test result: ok. 9 passed; 0 failed
```

---

## Using the Library

### Encrypt a sensor payload

```rust
use rustguard_pap::PacketBuilder;

// Pre-shared key (128-bit, provisioned at manufacture)
let key = [0x42u8; 16];

// Create a builder (initial sequence counter = 0)
let mut builder = PacketBuilder::new(key, 0);

// Encrypt a 32-byte sensor reading
let payload = b"Temperature: 22.4C | Hum: 65%  ";
let packet = builder.build_packet(payload, 0x0001, 1, 1);
// packet is a heapless::Vec<u8, 552> — no heap allocation
// wire size = payload.len() + 40 bytes overhead
```

### Decrypt and verify

```rust
use rustguard_pap::PacketBuilder;

let key = [0x42u8; 16];
let rx = PacketBuilder::new(key, 0);
let mut plaintext = [0u8; 512];

match rx.unwrap_packet(&packet, 0, &mut plaintext) {
    Ok(len)  => println!("Received: {}", core::str::from_utf8(&plaintext[..len]).unwrap()),
    Err(e)   => eprintln!("Rejected: {:?}", e),
}
```

### Raw ASCON-128 AEAD

```rust
use rustguard_core::{ascon_aead_encrypt, ascon_aead_decrypt};

let key:   [u8; 16] = [0x42; 16];
let nonce: [u8; 16] = [0xAA; 16];
let pt = b"IoT sensor data";
let ad = b"device_id=0x0001";

let mut ct  = [0u8; 15];
let mut tag = [0u8; 16];
ascon_aead_encrypt(&key, &nonce, ad, pt, &mut ct, &mut tag);

let mut rec = [0u8; 15];
let ok = ascon_aead_decrypt(&key, &nonce, ad, &ct, &mut rec, &tag);
assert!(ok);
assert_eq!(&rec, pt);
```

---

## RustGuard-PAP Packet Format

```
 0        4        8       20              20+N    36+N
 ┌────────┬────────┬────────┬──────────────┬────────┐
 │ Header │  Seq   │ Nonce  │  Ciphertext  │  Tag   │
 │  (4 B) │  (4 B) │ (16 B) │    (N B)     │ (16 B) │
 └────────┴────────┴────────┴──────────────┴────────┘
 └── Associated Data (authenticated, not encrypted) ──┘
```

| Field | Size | Protection |
|---|---|---|
| Version / Type / Device ID | 4 B | Authenticated |
| Sequence counter | 4 B | Authenticated (replay guard) |
| Nonce | 16 B | Transmitted in plaintext |
| Ciphertext | N B | Encrypted + Authenticated |
| ASCON-128 tag | 16 B | Integrity-protected |
| **Total overhead** | **40 B** | Fixed, independent of payload |

---

## Performance (x86-64 Host Benchmarks)

> All measurements: Rust 1.75, `opt-level=3`, `lto=true`, `codegen-units=1`,
> `N=10,000` iterations, 200 warmup, `std::time::Instant`.
> **These are host benchmarks — not ARM Cortex-M cycle counts.**
> See [`results/raw/benchmark_x86_64.txt`](results/raw/benchmark_x86_64.txt) for full data.

| Payload | Encrypt (mean ± σ) | Decrypt | PAP packet |
|---|---|---|---|
| 8 B | 260.4 ± 230.1 ns | 275.0 ± 237.1 ns | 584.3 ns |
| 32 B | 349.8 ± 191.5 ns | 377.9 ± 259.5 ns | 700.0 ns |
| 64 B | 470.6 ± 306.9 ns | 479.1 ± 202.2 ns | 892.0 ns |
| 128 B | 713.8 ± 351.5 ns | 734.0 ± 322.8 ns | 1,224 ns |
| 512 B | 2,100.4 ± 553.2 ns | 2,136.7 ± 591.3 ns | 3,424 ns |

**Permutation latency:** p¹² = 86.1 ns · p⁶ = 58.5 ns

**Binary size:** 11,605 bytes text segment (x86-64 release, LTO)

---

## Reproducing Results

### Run benchmarks

```bash
bash scripts/run_benchmarks.sh
```

### Regenerate all figures

```bash
pip install matplotlib numpy
python3 scripts/generate_figures.py
```

Figures are written to `results/figures/`.

### Compile the paper

```bash
cd paper
pdflatex -interaction=nonstopmode RustGuard_IEEE_Paper.tex
pdflatex -interaction=nonstopmode RustGuard_IEEE_Paper.tex  # second pass for refs
```

---

## Security Properties

- **Zero unsafe code:** `#![forbid(unsafe_code)]` — compile-time enforced
- **No heap allocation:** all buffers are `heapless::Vec` on the stack
- **Branchless S-box:** no data-dependent branches or table lookups
- **Constant-time tag comparison:** uses `subtle::ConstantTimeEq`
- **Automatic key erasure:** `zeroize::Zeroize` derived on `State` struct
- **Replay protection:** monotonic sequence counter in PAP protocol

---

## Citation

If you use this work, please cite:

```bibtex
@inproceedings{ali2025rustguard,
  title     = {{RustGuard}: Design and Software Verification of a Memory-Safe,
               no\_std {ASCON-128} Authenticated Encryption Library
               for {IoT} Packet Security},
  author    = {Ali, Taha Hunaid and Khan, Farhan},
  booktitle = {Proceedings of the IEEE Computing and Communication Workshop
               and Conference (CCWC)},
  year      = {2025},
  institution = {Habib University, Karachi, Pakistan}
}
```

---

## License

MIT License — see [LICENSE](LICENSE).

---

## Authors

**Taha Hunaid Ali** — Computer Science, Habib University
`ta08451@st.habib.edu.pk`

**Farhan Khan** — Assistant Professor, Electrical and Computer Engineering, Habib University
`farhan.khan@sse.habib.edu.pk`
