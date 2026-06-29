# Contributing to RustGuard

RustGuard is a research repository. The cipher is the device under test for a
study of whether Rust's source-level constant-time guarantees survive
compilation to embedded silicon, and what memory safety costs in cycles versus
C/asm. Contributions should preserve that measurement integrity.

## Hard rules

1. **No unsafe in the crypto crates.** `#![forbid(unsafe_code)]` is enforced on
   `rustguard-core` and `rustguard-pap`. The only permitted `unsafe` is isolated
   MMIO in the firmware crates.

2. **No heap allocation.** Buffers use `heapless` or fixed-size arrays — no
   `Vec`, `Box`, or `String` from `std`.

3. **Never weaken the KATs.** `rustguard-core/tests/kat.rs` checks the cipher
   against the published ASCON reference. If a change makes a KAT fail, the
   change is wrong — do not edit the vectors to make it pass.

4. **Never enable `tvla-leaky-control` by default.** It is a deliberately
   variable-time positive control for the side-channel experiment. It stays
   gated behind the feature flag.

5. **No fabricated results.** Do not commit synthetic traces, cycle counts, or
   t-statistics presented as measured. Measured data comes from the bench only.
   This is the failure mode the project was redirected to avoid.

## Host checks before submitting

```sh
cargo test -p rustguard-core -p rustguard-pap        # correctness + protocol
cargo build -p rustguard-core --features tvla-leaky-control
cargo fmt --all
cargo clippy -p rustguard-core -p rustguard-pap --all-targets -- -D warnings
```

## Firmware cross-compilation

The firmware crates are excluded from the host workspace and build for
`thumbv7em-none-eabihf` with their own `.cargo/config.toml`:

```sh
rustup target add thumbv7em-none-eabihf
cd firmware-tm4c && cargo check --release                       # perf
cd firmware-tm4c && cargo check --release --features timing     # timing harness
cd firmware-tm4c && cargo check --release --features "timing leaky"
```

## Areas open for contribution

- Additional ASCON KAT vectors and differential test harnesses
- pqm4 / ascon-c baseline integration (`baseline-c/`)
- Optimization-level sweeps and additional timing-leakage experiments
- Porting the timing harness to other Cortex-M4 boards (only UART is board-specific)
- Boolean masking for second-order resistance
- (future, needs a capture rig) power/EM side-channel analysis

See `docs/CLAUDE_CODE_HANDOFF.md` for current project state and the task list.
