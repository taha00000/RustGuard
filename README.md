# RustGuard

A memory-safe, `no_std` ASCON-128 implementation used as the device under test
for a study of **whether Rust's source-level constant-time guarantees survive
compilation to embedded silicon — measured as timing leakage on the chip itself —
and what memory safety costs in cycles versus C and hand-optimized assembly.**

This is a research repository, not just a library. The cipher is a means to an
end: a clean, verified, memory-safe ASCON whose constant-time behavior and
performance are measured on a real Cortex-M4, using only the ARM core's built-in
cycle counter — **no oscilloscope or ChipWhisperer required**, so anyone with the
same dev board can reproduce the result.

## Research question

NSA/CISA now recommend memory-safe languages for security-critical firmware, and
Rust's `#![forbid(unsafe_code)]` plus the `subtle` crate advertise constant-time
crypto. But constant-time at the source level is routinely undone by the compiler.
RustGuard measures, on physical hardware:

1. **Does it hold?** A dudect-style timing-leakage test (Reparaz, Balasch &
   Verbauwhede, DATE 2017): fixed-vs-random inputs, a Welch t-test over **DWT
   cycle counts** measured on-chip, comparing the constant-time implementation
   against a deliberately leaky positive control, across optimization levels.
2. **What does safety cost?** Real DWT cycle counts on a TM4C123, Rust vs the
   ASCON reference C and pqm4 assembly, on the same board and toolchain.

**Cross-silicon:** the harness is portable across Cortex-M4 parts (TI, STM32,
Nordic) — running it on several boards shows the finding is not an artifact of one
microarchitecture. The only board-specific code is UART init; DWT is core-standard.

**Scope (honest limitation):** this detects *timing* leakage, not *power/EM*
leakage. Constant-time code can still leak through power consumption. A power/EM
TVLA study (which needs a ChipWhisperer-class capture rig) is future work; the
claim here is specifically about the timing channel — fully measurable on a bare
board, and fully reproducible.

## What's verified today

- **Correctness.** The AEAD matches the published ASCON-128 reference for 8
  known-answer vectors (`rustguard-core/tests/kat.rs`). Real KATs, not round-trip
  self-consistency checks.
- **Memory safety.** `rustguard-core` and `rustguard-pap` are
  `#![no_std] #![forbid(unsafe_code)]`. The only `unsafe` is isolated MMIO in the
  firmware crate.
- **Protocol robustness.** `rustguard-pap` uses a reboot-robust nonce
  (epoch ‖ counter ‖ uid-hash) preventing nonce-reuse-on-reset. See
  `docs/protocol_security.md`.
- **Pipeline.** `python analysis/selftest.py` runs the entire parse → plot →
  timing-leakage chain on synthetic data and checks the t-test flags the leaky
  control and clears constant-time timing. Works before any hardware arrives.

## Layout

| Path | What |
|---|---|
| `rustguard-core` | ASCON-128 AEAD + ASCON-HASH, `no_std`, verified KATs |
| `rustguard-pap` | reboot-robust packet authentication protocol |
| `firmware-tm4c` | perf benchmark (default) and dudect timing harness (`--features timing`) |
| `capture` | `collect_timing.py` — drives the timing harness over UART |
| `analysis` | `parse_perf`, `plot_perf`, `dudect` (Welch t-test + histograms), `make_figures`, `selftest` |
| `docs` | protocol security, hardware setup, BOM, experiment runbook |

## Quick start (host, no hardware)

```sh
cargo test -p rustguard-core -p rustguard-pap   # correctness + protocol
pip install -r analysis/requirements.txt
python analysis/selftest.py                     # dry-run the full figure pipeline
```

Hardware experiments and the exact shopping list: `docs/hardware_bom.md` and
`docs/experiment_runbook.md`. The repository deliberately ships **no** measured
numbers — those are produced on the bench, by design.

## Status

Active research, redirected from an earlier implementation-only draft, and scoped
to hardware you can validate on a single Cortex-M4. See
`docs/CLAUDE_CODE_HANDOFF.md` for current state and task list.

## License

MIT — see [LICENSE](LICENSE).
