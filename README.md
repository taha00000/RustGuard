# RustGuard: A Memory-Safe, Constant-Time ASCON-128 Implementation

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-no__std-lightgrey.svg)
![Architecture](https://img.shields.io/badge/arch-ARM%20Cortex--M-orange.svg)

**RustGuard** is the first formally evaluated `no_std` Rust implementation of ASCON-128 authenticated encryption, specifically engineered for packet-level security on ARM Cortex-M microcontrollers. This repository provides the official implementation accompanying the IEEE research paper: *"RustGuard: A Memory-Safe, Constant-Time ASCON-128 Implementation for Authenticated IoT Packet Security on ARM Cortex-M Microcontrollers."*

## Architecture

This Cargo Workspace is strictly organized into three distinct layers to ensure maximum reusability and security:

1. **`rustguard-core`**: The highly optimized, branchless `no_std` implementation of the ASCON-128 AEAD suite. Ensures constant-time execution via `subtle::Choice` and automated memory erasure via `zeroize`.
2. **`rustguard-pap`**: The minimal IoT Packet Authentication Protocol (PAP). Frames payloads safely matching memory constraints avoiding heap allocations via `heapless`. Includes replay-attack sequence validation.
3. **`rustguard-hal`**: The firmware integration demonstrating bare-metal deployment on Cortex-M architectures (STM32L476).

## Benchmarks & Results

As validated in the paper, RustGuard achieves state-of-the-art performance natively on Cortex-M4 operating at 80 MHz:
- **Speed**: 8.3 cycles/byte (only 5.1% overhead vs C implementations)
- **Memory**: 1.2 KB RAM / 6.8 KB Flash
- **Energy**: 0.48 µJ per 64-byte payload
- **Security Check**: Verified free from first-order side-channel leakage up to 10,000 power traces via TVLA.

## Usage and Reproducibility

### 1. Library Integration
To use the pure cryptographic logic in another embedded project, include `rustguard-core` and `rustguard-pap` in your `Cargo.toml`. Since they are pure `no_std`, they compile natively for any architecture.

```toml
[dependencies]
rustguard-pap = { path = "path/to/rustguard-pap" }
```

### 2. Running Hardware Benchmarks (STM32L476)
To reproduce the empirical DWT cycle counting discussed in Section VII.A:

```bash
# Enter the HAL directory
cd rustguard-hal

# Build the firmware release targeting the Cortex-M4F
cargo build --release --target thumbv7em-none-eabihf

# Flash onto your STM32L476 board via probe-run
cargo run --release --target thumbv7em-none-eabihf
```

Connect to the UART output (115200 baud) to monitor live benchmark reporting of the PacketBuilder.

## Security Guarantees
- **Memory Safety**: Written entirely in Rust with `no_std` preventing buffer overflows, use-after-free, and dangling pointer vulnerabilities inherent in IoT deployments.
- **Timing Attacks**: The `SubBytes` operations inside the ASCON block employ bit-slicing logic wrapping critical values in `subtle` types. At compilation, conditional loops are evaluated into constant-time sequential logic.
- **Cold Boot**: Automated stack erasure via `zeroize(drop)` purges key material immediately following initialization completion.
