# Handoff to Claude Code — RustGuard

You are picking up a research project mid-flight. This document is the single
source of truth for what it is, what state it's in, and what to do next. Read it
fully before touching files.

## TL;DR

RustGuard was redirected from a (rejected-tier) "I implemented ASCON in Rust"
paper into a real research contribution, then scoped to hardware the owner
actually has: **a single Cortex-M4 (TM4C123), no oscilloscope or ChipWhisperer.**
The research question:

> Rust's `#![forbid(unsafe_code)]` + the `subtle` crate promise constant-time
> crypto. Does that guarantee survive compilation to embedded silicon — detectable
> as *timing* leakage measured on-chip with the DWT cycle counter (dudect method,
> Reparaz et al. DATE 2017) — and what does memory safety cost in cycles versus
> C/asm? Cross-checked across multiple Cortex-M4 boards.

Power/EM side-channel analysis (which needs a ChipWhisperer-class rig) is
explicitly **future work** and out of scope. The owner is **taha00000**; he runs
the hardware, makes the research calls, and authors all commits.

## Repository layout

```
rustguard-core/        ASCON-128 AEAD + HASH, no_std, forbid(unsafe). VERIFIED.
  tests/kat.rs         REAL NIST KATs (8 vectors), all passing
  src/lib.rs           gated ascon_aead_decrypt_variabletime = leaky control
rustguard-pap/         reboot-robust nonce (epoch+counter+uid); integration tests
firmware-tm4c/         default = perf benchmark (real DWT cycles over UART)
                       --features timing       = dudect timing-leakage harness
                       --features "timing leaky" = harness w/ variable-time compare
capture/               collect_timing.py — drives the harness over UART (pyserial)
analysis/              parse_perf, plot_perf, dudect (Welch t + histograms),
                       make_figures (orchestrator), selftest (synthetic E2E)
docs/                  protocol_security, hardware_setup, hardware_bom, runbook
```

## What is DONE and trustworthy (host-verifiable, no hardware)

```sh
cargo test -p rustguard-core -p rustguard-pap      # KATs + protocol, green
cargo build -p rustguard-core --features tvla-leaky-control
pip install -r analysis/requirements.txt
python -m pytest analysis/tests                    # t-test + parser unit tests
python analysis/selftest.py                        # full pipeline on synthetic data
```

- The cipher is correct: byte-for-byte against the published ASCON reference for 8
  vectors. Do not "optimize" the permutation without re-running KATs.
- The leaky variant is a deliberate positive control. It must NEVER be enabled in
  a production/default build. Keep it gated.
- `selftest.py` proves the parse→plot→timing-leakage chain works and asserts the
  t-test separates leaky from constant-time. Its figures are watermarked and land
  in gitignored `results/_demo/` — never a result.

Firmware builds (need `rustup target add thumbv7em-none-eabihf`):
```sh
cd firmware-tm4c
cargo check --release                       # perf
cargo check --release --features timing     # timing harness
cargo check --release --features "timing leaky"
```

## What is HARDWARE-GATED (code is real; results need the TM4C)

- `firmware-tm4c` perf mode → real cycle counts over an external USB-UART dongle
  (the ICDI fix is in docs/hardware_setup.md).
- `firmware-tm4c --features timing` + `capture/collect_timing.py` → the timing-
  leakage samples. Real serial driver; runs when a flashed board is connected.
- These are not stubs and must not be "completed" with fake/synthetic data.

## What is intentionally NOT here

- No measured cycle counts, no timing `.npz`, no figures. Those come from the
  bench. If you find yourself about to generate data to "finish" the repo — stop.
  Synthetic results dressed as measurements are exactly what sank the old paper.
- No power-capture firmware or ChipWhisperer driver (removed in the M4-only
  redirect; recoverable from git history if a capture rig is ever obtained).

## Your task list (in order)

1. Confirm the host-verifiable parts build/test green. Fix toolchain trivia only;
   if a KAT fails after an edit, revert — you broke the cipher.
2. Help Taha bring up the board: flash perf, capture, parse; then flash the timing
   harness (leaky first, then safe), collect, and run `analysis/dudect.py`.
3. Optimization-level sweep (the core novelty) and, optionally, port the timing
   harness to a second M4 board (only uart_init/getc/putc are board-specific).
4. Paper changes come AFTER the code is consistent and at least the perf and the
   leaky-vs-safe timing results are real. Taha signals when to start on the .tex.

## Commits

Author commits as Taha (git config: name `taha00000`, his email). His work, his
repo, his author line — normal and correct. Write honest messages describing what
changed. The tooling being AI-assisted is fine and need not be hidden, nor
misrepresented — describe the change, not who typed it.

## Hard boundaries (do not cross, regardless of deadline pressure)

- Never generate, interpolate, or "placeholder" experimental results (cycle
  counts, timing samples, t-statistics) and present them as measured.
- Never enable the leaky control in a default build.
- Never weaken or skip the KATs to make a refactor pass.
- If asked to make results look better than the data supports, decline and say
  why. A reviewer catching fabricated data ends the paper and the reputation.

## Quick orientation

```sh
cargo test -p rustguard-core -p rustguard-pap     # should be green
python analysis/selftest.py                       # should print SELF-TEST PASSED
cat docs/experiment_runbook.md                    # end-to-end bench workflow
cat docs/hardware_bom.md                          # what to buy (just M4 + dongle)
```
