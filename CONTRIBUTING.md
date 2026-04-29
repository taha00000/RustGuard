# Contributing to RustGuard

Thank you for your interest in contributing!

## Requirements for all contributions

1. **No unsafe code.** `#![forbid(unsafe_code)]` is enforced on all crates.
   PRs that introduce `unsafe` blocks will not be merged.

2. **No heap allocation.** All buffers must use `heapless` or fixed-size
   arrays. No `Vec`, `Box`, or `String` from `std`.

3. **Tests required.** Any new functionality must be accompanied by tests.
   Run `cargo test --workspace` before submitting.

4. **Clippy clean.** Run `cargo clippy --workspace --all-targets -- -D warnings`
   and resolve all warnings.

5. **Formatted.** Run `cargo fmt --all` before committing.

## Development workflow

```bash
# Clone
git clone https://github.com/taha00000/RustGuard.git
cd rustguard

# Test
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Format
cargo fmt --all

# Check embedded cross-compilation
rustup target add thumbv7em-none-eabihf
cargo check -p rustguard-core --target thumbv7em-none-eabihf
cargo check -p rustguard-pap  --target thumbv7em-none-eabihf
```

## Areas open for contribution

- ARM Cortex-M4 hardware benchmarks (requires STM32L476 or nRF52840)
- ChipWhisperer TVLA side-channel evaluation
- ASCON-80pq variant implementation
- Boolean masking for second-order DPA resistance
- Additional target HAL integrations (nRF52840, ESP32-S3)
