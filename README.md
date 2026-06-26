# RustGuard

A memory-safe, `no_std` ASCON-128 implementation used as the device under test
for a study of **whether Rust's source-level constant-time guarantees survive
compilation to embedded silicon — and what memory safety costs in cycles versus
C and hand-optimized assembly, measured on real hardware.**

This is a research repository, not just a library. The cipher is a means to an
end: a clean, verified, memory-safe ASCON whose constant-time behavior can be
measured on a ChipWhisperer and whose performance can be compared head-to-head
with the established C/asm baselines on the same boards.

## Research question

NSA/CISA now recommend memory-safe languages for security-critical firmware, and
Rust's `#![forbid(unsafe_code)]` plus the `subtle` crate advertise constant-time
crypto. But constant-time at the source level is routinely undone by the compiler
and by data-dependent timing on real cores. RustGuard measures, on physical
hardware:

1. **Does it hold?** TVLA (fixed-vs-random Welch t-test) on an STM32F3/CW308,
   comparing the constant-time implementation against a deliberately leaky
   positive control, across optimization levels.
2. **What does safety cost?** Real DWT cycle counts on a TM4C123, Rust vs the
   ASCON reference C and pqm4 assembly, on the same board and toolchain.

## What's verified today

- **Correctness.** The AEAD matches the published ASCON-128 reference for 8
  known-answer vectors (`rustguard-core/tests/kat.rs`). These are real KATs, not
  round-trip self-consistency checks.
- **Memory safety.** `rustguard-core` and `rustguard-pap` are
  `#![no_std] #![forbid(unsafe_code)]`. The only `unsafe` in the project is
  isolated MMIO in the firmware crates.
- **Protocol robustness.** `rustguard-pap` uses a reboot-robust nonce
  (epoch ‖ counter ‖ uid-hash) that prevents the nonce-reuse-on-reset failure of
  naive counter-only designs. Argument in `docs/protocol_security.md`.

## Layout

| Path | What |
|---|---|
| `rustguard-core` | ASCON-128 AEAD + ASCON-HASH, `no_std`, verified KATs |
| `rustguard-pap` | reboot-robust packet authentication protocol |
| `firmware-tm4c` | performance firmware (real DWT cycles over UART) |
| `firmware-stm32-tvla` | side-channel target (GPIO trigger, simpleserial-style) |
| `baseline-c` | pqm4 + ascon-c baselines for same-board comparison |
| `capture` | ChipWhisperer fixed-vs-random acquisition |
| `analysis` | Welch's t-test TVLA + perf parsing |
| `docs` | protocol security, hardware setup, experiment runbook |

## Quick start (host)

```sh
cargo test -p rustguard-core -p rustguard-pap   # correctness + protocol
```

Hardware experiments: see `docs/experiment_runbook.md` and
`docs/hardware_setup.md`. The repository deliberately ships **no** trace data or
measured numbers — those are produced on the bench, by design, so every reported
result is real.

## Status

Active research, redirected from an earlier implementation-only draft. See
`docs/CLAUDE_CODE_HANDOFF.md` for the current state and task list.

## License

MIT — see [LICENSE](LICENSE).
