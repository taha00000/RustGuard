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
crypto. But constant-time at the source level is routinely undone by the compiler,
and a source-level guarantee says nothing about the binary that actually ships or
the silicon it runs on. RustGuard asks: **does the constant-time property hold all
the way from source to silicon, and where does it break?** — and answers it by
*triangulating three independent checks* of the same property, each at a different
level, all on hardware you can buy for $20:

1. **Source** — exhaustive differential correctness against the ASCON reference
   (the `tests/kat.rs` known-answer vectors), establishing the implementation is
   the cipher it claims to be.
2. **Binary** — a control-flow census of the compiled `thumbv7em` image
   (`analysis/ct_binary.py` over `rust-objdump`): per function, the conditional
   branches / IT blocks / variable-latency ops that can carry data-dependent
   timing. The safe-vs-leaky *differential* localizes secret-dependent branching
   in the actual deployed artifact, across optimization levels.
3. **Silicon** — a dudect-style timing-leakage test on the TM4C123 (Reparaz,
   Balasch & Verbauwhede, DATE 2017): fixed-vs-random inputs, a Welch t-test over
   on-chip **DWT cycle counts**, against a deliberately leaky positive control.

The interesting result is where the three levels **agree or disagree** as the
optimizer rewrites the code. Alongside this, RustGuard measures **what memory
safety costs** — Rust vs the ASCON reference C and pqm4 assembly, same board, same
toolchain.

**Cross-silicon:** the harness is portable across Cortex-M4 parts (only UART init
is board-specific; DWT is core-standard), so the finding can be shown not to be an
artifact of one microarchitecture.

**Scope (honest limitations):** the silicon leg detects *timing* leakage, not
*power/EM* (a ChipWhisperer-class rig, out of scope, is future work). The binary
leg is a control-flow census + differential — a practical screen validated by its
controls, **not** a sound taint-tracking proof (cf. BINSEC/Rel); a machine-checked
proof (Kani/Verus) is the documented next milestone. The contribution is the
*triangulated, reproducible-on-a-bare-board methodology* and what it reveals, not
any single tool.

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
- **Binary census.** `analysis/ct_binary.py` disassembles the compiled
  `thumbv7em` crypto and reports that the variable-time decrypt carries extra
  secret-dependent branches the constant-time one does not — the leak, localized
  in the actual binary (runs in CI on every push).
- **Pipeline.** `python analysis/selftest.py` runs the entire chain (perf, timing
  t-test, binary census) on synthetic data and checks the controls separate as
  they must. Works before any hardware arrives.

## Layout

| Path | What |
|---|---|
| `rustguard-core` | ASCON-128 AEAD + ASCON-HASH, `no_std`, verified KATs |
| `rustguard-pap` | reboot-robust packet authentication protocol |
| `firmware-tm4c` | perf benchmark (default) and dudect timing harness (`--features timing`) |
| `ct-probe` | thumbv7em staticlib exposing each primitive as a symbol for binary analysis |
| `capture` | `collect_timing.py` — drives the timing harness over UART |
| `analysis` | `parse_perf`, `plot_perf`, `dudect`, `ct_binary`, `opt_sweep`, `tables`, `make_figures`, `selftest` |
| `docs` | protocol security, hardware setup, BOM, experiment runbook, status |

`make_figures.py` produces **7 figures** (throughput, memory-safety overhead,
permutation cost, timing histograms, t-statistic convergence, optimization-level
sweep, binary control-flow census) and **5 tables** (cycle counts, timing summary,
code size, opt sweep, binary census) as Markdown + LaTeX. `selftest.py` builds and
checks all of them on synthetic, watermarked data so the pipeline is verifiable
before any hardware arrives.

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

Active research, scoped to hardware you can validate on a single Cortex-M4. See
`docs/STATUS.md` for current state, what is verified, and what needs the bench.

## License

MIT — see [LICENSE](LICENSE).
