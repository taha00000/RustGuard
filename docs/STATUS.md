# Project status

What this repo is, what is verified, and what still needs the bench. Kept current
so anyone (including future-me) can pick it up without guessing.

## What it is

A memory-safe `no_std` ASCON-128 used as the device under test for measuring,
on a single Cortex-M4 (TM4C123, no oscilloscope):

1. whether the source-level constant-time property survives compilation, detected
   as *timing* leakage on-chip with the DWT cycle counter (dudect method); and
2. the cycle cost of memory safety, Rust vs the C reference and pqm4 assembly.

Power/EM side-channel analysis (needs a capture rig) is explicitly future work.

## Verified today (host, no hardware)

```sh
cargo test -p rustguard-core -p rustguard-pap      # KATs + protocol, green
cargo build -p rustguard-core --features tvla-leaky-control
pip install -r analysis/requirements.txt
python -m pytest analysis/tests                    # analysis unit tests
python analysis/selftest.py                        # full pipeline on synthetic data
```

- Cipher correctness: byte-for-byte against the published ASCON reference for 8
  known-answer vectors (`rustguard-core/tests/kat.rs`). Do not change the
  permutation without re-running KATs.
- Memory safety: `rustguard-core` and `rustguard-pap` are
  `#![no_std] #![forbid(unsafe_code)]`; the only `unsafe` is isolated MMIO in the
  firmware crate.
- The leaky variant is a deliberate positive control, gated behind a feature. It
  must never be enabled in a production/default build.

Firmware (needs `rustup target add thumbv7em-none-eabihf`):
```sh
cd firmware-tm4c
cargo check --release                       # perf benchmark
cargo check --release --features timing     # timing-leakage harness
cargo check --release --features "timing leaky"
```

## Hardware-gated (code is real; results need the TM4C)

- perf mode → real cycle counts over an external USB-UART dongle.
- `--features timing` + `capture/collect_timing.py` → timing-leakage samples.
- These are not stubs and are never to be "completed" with synthetic data.

## Boundaries

- No generated, interpolated, or placeholder experimental results presented as
  measured. The synthetic `results/_demo/` output of `selftest.py` is watermarked
  and never enters `results/figures/`.
- Don't weaken or skip the KATs to make a refactor pass.

## Where to look

- `docs/experiment_runbook.md` — end-to-end bench workflow.
- `docs/hardware_bom.md` — what to buy (just an M4 board + USB-UART dongle).
- `docs/hardware_setup.md` — wiring and the UART capture fix.
- `docs/protocol_security.md` — the reboot-robust nonce argument.
