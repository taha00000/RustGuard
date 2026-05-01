# RustGuard

**Memory-Safe `no_std` ASCON-128 Authenticated Encryption for IoT Packet Security**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![no_std](https://img.shields.io/badge/no__std-%E2%9C%93-lightgrey.svg)](https://docs.rust-embedded.org/book/intro/no-std.html)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)
[![Tests](https://img.shields.io/badge/tests-24%2F24%20passed-brightgreen.svg)](#testing)

---

> **Research Paper:** *RustGuard: Memory-Safe ASCON-128 Authenticated Encryption for IoT Packet Security — Design, Verification, and Embedded Evaluation on ARM Cortex-M4F*
> [[PDF — blind submission]](paper/ndss/rustguard_ndss_BLIND.pdf) · [[LaTeX source]](paper/ndss/rustguard_ndss_BLIND.tex)

---

## Overview

RustGuard implements **ASCON-128** — the algorithm selected by NIST in February 2023 as the lightweight cryptography standard (NIST IR 8454) — together with a compact **IoT Packet Authentication Protocol (RustGuard-PAP)**, in pure safe Rust with zero heap allocation.

The implementation is verified on **real ARM Cortex-M4F hardware** (TM4C123GH6PM, EK-TM4C123GXL LaunchPad), achieving 2,303 cycles (143.9 µs) for a 32-byte payload at 16 MHz.

### Key Properties

| Property | Value |
|---|---|
| Algorithm | ASCON-128 AEAD + ASCON-HASH (NIST IR 8454) |
| Key / Nonce / Tag size | 128 bits each |
| `unsafe` code | **Zero** (`#![forbid(unsafe_code)]`) |
| `std` dependency | **None** (`#![no_std]`) |
| Heap allocation | **None** (`heapless` stack buffers) |
| Test suite | **24/24 tests pass** |
| x86-64 text segment | **11,605 bytes** (release, LTO) |
| TM4C123 Flash usage | **39,520 bytes** (15.4% of 256 KB) |

---

## Repository Structure

```
RustGuard/
├── rustguard-core/              # ASCON-128 AEAD + ASCON-HASH
│   ├── src/
│   │   ├── lib.rs               # Permutation, AEAD, HASH (337 lines)
│   │   └── tests.rs             # 15 unit tests
│   └── Cargo.toml
│
├── rustguard-pap/               # IoT Packet Authentication Protocol
│   ├── src/lib.rs               # PacketBuilder, unwrap_packet
│   ├── tests/integration.rs     # 9 integration tests
│   └── Cargo.toml
│
├── rustguard-hal-tiva/          # TM4C123GH6PM bare-metal benchmark firmware
│   ├── src/main.rs              # DWT cycle counting, UART output, LED
│   ├── build.rs                 # Linker script helper
│   ├── memory.x                 # 256KB Flash / 32KB SRAM linker script
│   ├── .cargo/config.toml       # thumbv7em-none-eabihf target
│   └── Cargo.toml
│
├── paper/
│   └── ndss/
│       ├── rustguard_ndss_BLIND.pdf   # ← SUBMIT THIS to NDSS 2027
│       ├── rustguard_ndss_BLIND.tex   # LaTeX source (anonymous)
│       ├── rustguard_ndss_full.tex    # Full version with author info
│       └── ndss.cls                   # NDSS conference class file
│
├── overleaf/                    # Drop into Overleaf → compile immediately
│   ├── rustguard_ndss_BLIND.tex
│   ├── ndss.cls
│   ├── figures/                 # All 8 figures (PDF + PNG)
│   └── README_OVERLEAF.txt
│
├── results/
│   ├── figures/                 # All 8 publication figures (PDF + PNG)
│   │   ├── fig1_latency.*       # AEAD latency vs payload
│   │   ├── fig2_throughput.*    # KB/s throughput scaling
│   │   ├── fig3_permutation.*   # p6 vs p12 bar chart
│   │   ├── fig4_breakdown.*     # Phase decomposition
│   │   ├── fig5_overhead.*      # PAP 40-byte overhead
│   │   ├── fig6_tests.*         # 24/24 test results
│   │   ├── fig7_codesize.*      # nm binary analysis
│   │   └── fig8_hw_comparison.* # x86-64 vs TM4C123 latency
│   └── raw/
│       ├── benchmark_x86_64.txt # x86-64 measurements (N=10,000)
│       └── benchmark_tm4c.txt   # TM4C123 measurements (DWT, N=500)
│
├── scripts/
│   ├── generate_figures.py      # Reproduce all figures from raw data
│   ├── parse_hw_results.py      # Parse TM4C UART output into structured data
│   └── run_benchmarks.sh        # Run x86-64 benchmark suite
│
├── Cargo.toml                   # Workspace (core + pap only)
├── LICENSE                      # MIT
├── SECURITY.md
├── CONTRIBUTING.md
└── README.md
```

---

## Quick Start

### Build and Test (x86-64)

```bash
git clone https://github.com/taha00000/RustGuard.git
cd RustGuard
cargo test --workspace --release
```

Expected: **24/24 tests pass**

### Build Embedded Firmware (TM4C123GH6PM)

```bash
cd rustguard-hal-tiva
rustup target add thumbv7em-none-eabihf
cargo build --release --target thumbv7em-none-eabihf
rust-objcopy -O binary \
  target/thumbv7em-none-eabihf/release/rustguard-hal-tiva \
  out.bin
```

### Flash (Windows — LM Flash Programmer)

```cmd
cd "C:\Program Files (x86)\Texas Instruments\Stellaris\LM Flash Programmer"
LMFlash.exe -i ICDI -v -r "path\to\out.bin"
```

### Read Benchmark Output

Open serial port at **9600 baud, 8N1** (e.g. PuTTY on COM20).
Press RESET on board. LED blinks Red→Blue→Green on boot, then output streams.
Benchmarks complete in ~5 minutes. Copy output to `results/raw/benchmark_tm4c.txt`.

---

## Usage

### Encrypt a sensor payload

```rust
use rustguard_pap::PacketBuilder;

let key = [0x42u8; 16]; // 128-bit pre-shared key
let mut builder = PacketBuilder::new(key, 0);
let payload = b"Temperature: 22.4C  Humidity:65%";
let packet = builder.build_packet(payload, 0x0001, 1, 1);
// packet = heapless::Vec<u8, 552> — no heap, wire size = N + 40 bytes
```

### Decrypt and verify

```rust
let rx = PacketBuilder::new(key, 0);
let mut plaintext = [0u8; 512];
match rx.unwrap_packet(&packet, 0, &mut plaintext) {
    Ok(len) => { /* plaintext[..len] is verified payload */ }
    Err(e)  => { /* rejected: AuthenticationFailed / ReplayDetected */ }
}
```

---

## PAP Packet Format

```
┌────────┬────────┬──────────────────┬───────────────┬──────────┐
│ Header │  Seq   │      Nonce       │  Ciphertext   │   Tag    │
│  (4 B) │  (4 B) │     (16 B)       │    (N B)      │  (16 B)  │
└────────┴────────┴──────────────────┴───────────────┴──────────┘
└──── Associated Data (authenticated, not encrypted) ────┘
         Fixed overhead: 40 bytes
```

---

## Performance

### x86-64 (N=10,000, Rust 1.95, opt-level=3, LTO)

| Payload | Encrypt | Decrypt | PAP packet |
|---|---|---|---|
| 8 B  | 260 ± 230 ns | 275 ± 237 ns | 584 ns |
| 32 B | 350 ± 192 ns | 378 ± 260 ns | 700 ns |
| 512 B | 2,100 ± 553 ns | 2,137 ± 591 ns | 3,424 ns |

### TM4C123GH6PM ARM Cortex-M4F @ 16 MHz (N=500, DWT cycle counter)

| Payload | Cycles | Time (µs) | cyc/B |
|---|---|---|---|
| 8 B  | 1,556 | 97.3 | 194.5 |
| 32 B | 2,303 | 143.9 | 72.0 |
| 512 B | 17,243 | 1,077.7 | 33.7 |

**Permutation:** p⁶ = 249 cycles (15.6 µs) · p¹² = 499 cycles (31.2 µs)

---

## Reproducing Results

```bash
# Regenerate all 8 figures
pip install matplotlib numpy
python3 scripts/generate_figures.py

# Compile paper (on Overleaf or local LaTeX)
cd overleaf
pdflatex rustguard_ndss_BLIND.tex
pdflatex rustguard_ndss_BLIND.tex  # second pass for references
```

---

## Security Properties

- **Zero unsafe code:** `#![forbid(unsafe_code)]` — compile-time enforced
- **No heap:** all buffers are `heapless::Vec` on the stack
- **Branchless S-box:** zero data-dependent branches or table lookups
- **Constant-time tag comparison:** `subtle::ConstantTimeEq`
- **Automatic key erasure:** `zeroize::Zeroize` derived on `State`
- **Replay protection:** monotonic sequence counter in PAP

---

## Citation

```bibtex
@inproceedings{rustguard2027,
  title     = {{RustGuard}: Memory-Safe {ASCON}-128 Authenticated Encryption
               for {IoT} Packet Security---Design, Verification, and
               Embedded Evaluation on {ARM} {Cortex-M4F}},
  booktitle = {Proceedings of the Network and Distributed System Security
               Symposium (NDSS)},
  year      = {2027},
  note      = {Available: \url{https://github.com/taha00000/RustGuard}}
}
```

---

## License

MIT — see [LICENSE](LICENSE).
