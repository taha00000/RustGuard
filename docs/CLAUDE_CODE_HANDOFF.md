# Handoff to Claude Code — RustGuard Reframe-1

You are picking up a research project mid-flight. This document is the single
source of truth for what it is, what state it's in, and what to do next. Read it
fully before touching files.

## TL;DR

RustGuard is being redirected from a (rejected-tier) "I implemented ASCON in
Rust" paper into a real research contribution for a top-tier security venue. The
new research question is:

> Rust's `#![forbid(unsafe_code)]` + the `subtle` crate promise constant-time
> crypto. Does that guarantee survive compilation to embedded silicon, and what
> does memory safety cost in cycles versus C/asm — measured on real hardware?

The owner is **taha00000**. He runs the hardware, makes the research calls, and
authors all commits. Your job is to help land the code and keep the repo
consistent — not to manufacture results.

## Repository layout

```
rustguard-core/        ASCON-128 AEAD + HASH, no_std, forbid(unsafe). VERIFIED.
  src/lib.rs           includes a gated leaky tag-check for TVLA control
  tests/kat.rs         REAL NIST KATs (8 vectors), all passing
rustguard-pap/         revised protocol: reboot-robust nonce (epoch+counter+uid)
  tests/integration.rs reboot/replay/tamper coverage
firmware-tm4c/         performance firmware: real DWT cycles over UART
firmware-stm32-tvla/   TVLA target: GPIO trigger, simpleserial-style protocol
                       default build = constant-time DUT; --features leaky = control
baseline-c/            pqm4 + ascon-c baselines as submodules (setup.sh)
capture/               ChipWhisperer fixed-vs-random acquisition driver
analysis/              tvla.py (Welch t-test), parse_perf.py
docs/                  protocol_security.md, hardware_setup.md, experiment_runbook.md
```

## What is DONE and trustworthy (host-verifiable, no hardware needed)

These should compile and pass on any dev machine. Verify first thing:

```sh
cargo test -p rustguard-core      # KATs + (move tests/ unit tests if any)
cargo test -p rustguard-pap       # reboot/replay/tamper integration tests
cargo build -p rustguard-core --features tvla-leaky-control
```

- The cipher is correct: its outputs were checked byte-for-byte against the
  published ASCON reference for 8 vectors (empty, partial, full, multi-block,
  with/without AD). Do not "optimize" the permutation without re-running KATs.
- The leaky variant (`tvla-leaky-control`) is a deliberate positive control. It
  must NEVER be enabled in a production/default build. Keep it gated.

## What is COMPLETE but HARDWARE-GATED (code is real; results need the bench)

These are finished and runnable, but produce *results* only on Taha's hardware.
They are not stubs and must not be "completed" with fake/synthetic data.

- `firmware-tm4c` — flashes and measures real cycles. The UART fix (external
  USB-UART dongle, not ICDI) is documented in docs/hardware_setup.md.
- `firmware-stm32-tvla` — has `TODO(hardware)` markers on register addresses and
  `clocks_uart_init()` that must be confirmed against the actual STM32F303 part.
  These are the ONLY places to fill in hardware specifics.
- `capture/` and `analysis/` — the acquisition and TVLA t-test pipeline. Real,
  standard ChipWhisperer API. Runs when a CW + flashed target are connected.

## What is intentionally NOT here

- No power traces, no `.npz` results, no TVLA figures. Those come from Taha's
  capture runs. If you find yourself about to generate trace data to "finish"
  the repo — stop. Synthetic results are the exact failure that sank the old
  paper (it shipped pqm4-derived numbers dressed as measurements). The whole
  point of the redirect is that the numbers are real.

## Your task list (in order)

1. **Confirm the host-verifiable parts build and test green.** Fix any toolchain
   triviality (versions, imports). Do not change cipher logic; if a KAT fails
   after an edit, you broke something — revert.
2. **Clean the old repo state.** When this lands on top of the existing clone,
   delete the stale NDSS paper framing, the derived `benchmark_tm4c.txt` numbers,
   and the old figures. Preserve history (do not force-push over main); make a
   normal commit that removes them. See "Cleanup scope" below.
3. **Wire CI** (`.github/workflows/ci.yml` is provided) so core + pap test on
   every push, and the firmware crates at least `cargo check` for their target.
4. **Help Taha as he brings up hardware** — fill the `TODO(hardware)` markers in
   firmware-stm32-tvla once he confirms the board, build the two TVLA binaries,
   and run capture → analysis. Let him operate the bench; you handle the code.
5. **Paper changes come AFTER** the code is consistent and at least the perf
   numbers are real. Taha will signal when to start on the .tex.

## Cleanup scope (decided with the owner)

Remove: old NDSS `.tex` framing that claims four "contributions" that are just
artifacts; `results/raw/benchmark_tm4c.txt` and any file containing
pqm4-*derived* numbers; old figures built from derived data; the self-consistency
tests previously mislabeled as "nist_kat". Keep: the verified cipher, real KATs,
the firmware that genuinely measures, LICENSE, and anything the new experiment
uses. When in doubt, ask Taha — don't silently delete research history.

## Commits

Author commits as Taha (his git config: name `taha00000`, his email). This is
his work and his repo, so he is the author — that is normal and correct. Write
honest commit messages describing what changed. Do not fabricate authorship of
intellectual contributions: the research question, the hardware results, and the
claims are his. The tooling being AI-assisted is fine and need not be hidden, but
it also should not be misrepresented — just describe the change, not who typed it.

## Hard boundaries (do not cross, regardless of deadline pressure)

- Never generate, interpolate, or "placeholder" experimental results (traces,
  cycle counts, t-statistics) and present them as measured. Mark anything
  pending as pending.
- Never enable the leaky control in a default build.
- Never weaken or skip the KATs to make a refactor pass.
- If asked to make results look better than the data supports, decline and say
  why. A reviewer catching fabricated data ends the paper and the reputation;
  protecting Taha from that is part of the job.

## Quick orientation commands

```sh
cargo test -p rustguard-core -p rustguard-pap     # should be green
rg "TODO\(hardware\)"                             # the only place HW specifics go
cat docs/experiment_runbook.md                    # end-to-end bench workflow
```
