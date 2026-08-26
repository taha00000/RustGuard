# Project status

What this repo is, what is verified, and what still needs the bench. Kept current
so anyone (including future-me) can pick it up without guessing.

## Current scope (systematic ecosystem study)

Ten AEAD/MAC implementations from the Rust crypto ecosystem are evaluated for
timing leakage on real Cortex-M4 silicon, across four optimization levels and two
silicon vendors, with no lab equipment. The registry lives in `probes/`; adding a
crate means adding one entry there — the firmware, capture driver, and analysis
are generic. `results/figures/leakage_matrix.png` is the headline artifact.

The in-house ASCON is retained as the *validated control pair* (constant-time vs
deliberately variable-time), which is what proves the method detects a real leak
on this hardware. It is no longer presented as a performance contribution — its
124 cycles/byte is not competitive with optimized ASCON on Cortex-M4, and the C
baseline that claim would need was removed (it was built on the incorrect premise
that pqm4 ships ASCON; it does not).

## What it is

A memory-safe `no_std` ASCON-128 used as the device under test for a
source-to-silicon constant-time study on a single Cortex-M4 (TM4C123, no
oscilloscope). It checks the same code at four independent levels:

1. **proof** — Kani model-checking: safety (no panic/overflow/UB) + decryption
   recovery, machine-checked for all symbolic inputs (`docs/verification.md`);
2. **source** — differential correctness against the ASCON reference (KATs),
   including exact tag authentication;
3. **binary** — a control-flow census of the compiled thumb image
   (`analysis/ct_binary.py`), with the safe-vs-leaky differential localizing
   secret-dependent branches in the actual artifact;
4. **silicon** — dudect timing leakage on-chip via the DWT cycle counter.

Plus the cycle cost of memory safety (Rust vs C vs pqm4 asm). Power/EM analysis
(needs a capture rig) and full machine-checked tag-authentication proofs (beyond a
laptop SAT solver) are future work.

## Verified today (host, no hardware)

```sh
cargo test -p rustguard-core -p rustguard-pap      # KATs + protocol, green
cargo build -p rustguard-core --features tvla-leaky-control
pip install -r analysis/requirements.txt
python -m pytest analysis/tests                    # analysis unit tests
python analysis/selftest.py                        # full pipeline on synthetic data
cargo kani -p rustguard-core                        # 6/6 proofs (needs Kani; see verification.md)
```

- Cipher correctness: byte-for-byte against the published ASCON reference for 8
  known-answer vectors (`rustguard-core/tests/kat.rs`). Do not change the
  permutation without re-running KATs.
- Memory safety: `rustguard-core` and `rustguard-pap` are
  `#![no_std] #![forbid(unsafe_code)]`; the only `unsafe` is isolated MMIO in the
  firmware crate.
- The leaky variant is a deliberate positive control, gated behind a feature. It
  must never be enabled in a production/default build.
- Binary census: `ct-probe` builds for thumb and `analysis/ct_binary.py` reports
  the safe-vs-leaky branch differential (the leak, localized in the binary). Runs
  in CI on every push.
- Machine-checked: `cargo kani -p rustguard-core` verifies 6/6 proofs (safety +
  decryption recovery). See `docs/verification.md`. Runs in CI.

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

- `docs/verification.md` — the Kani machine-checked proofs (what's proven, scope).
- `docs/flashing.md` — one-command build+flash (`scripts/flash.ps1`, LM Flash Programmer).
- `docs/experiment_runbook.md` — end-to-end bench workflow.
- `docs/hardware_bom.md` — what to buy (just an M4 board + USB-UART dongle).
- `docs/hardware_setup.md` — wiring and the UART capture fix.
- `docs/protocol_security.md` — the reboot-robust nonce argument.
