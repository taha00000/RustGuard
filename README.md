# RustGuard

**A systematic, equipment-free constant-time evaluation of the Rust
cryptographic ecosystem on embedded silicon.**

Thirteen Rust implementations — nine AEADs and MACs, two public-key primitives,
and two deliberately-leaking controls — measured on two Cortex-M4
microcontrollers from different vendors, across four optimization levels and five
core-frequency configurations. **No oscilloscope, no ChipWhisperer, no trace
probe**: every measurement comes from the ARM core's own DWT cycle counter, so
anyone with a $20 development board can reproduce the whole study.

**257 captures, ~750,000 on-chip measurements, all real.** Nothing in
`results/` is generated, interpolated, or simulated.

## Results in one table

Every production implementation is cycle-invariant in every configuration. The
sharper statement is that in **108 of 130** verification captures and **116 of
127** keyed captures, the cycle count was *identical for every single trace* —
a spread of exactly zero. The captures that do vary are exactly the controls.

| finding | where |
|---|---|
| Nine AEAD/MAC and two public-key implementations show no input-dependent timing | `results/tables/leakage_matrix.md` |
| **LLVM removes a real timing leak at `-O2`** — identically on both vendors | `results/tables/leakage_matrix.md` |
| Flash wait states inflate cycles 4.3–27.2%, without creating data dependence | `tm4c_80MHz_*` vs `tm4c_O3_*` |
| Binary branch-counting: **8 false positives out of 10 flagged**, 0 missed | `results/tables/static_vs_measured.md` |
| Detection floor: a **0.49-cycle** mean difference is still caught | `results/tables/detection_floor.md` |

The draft paper is in [`paper/`](paper/).

## Why this is measurable without lab equipment

A Cortex-M4 has no data cache and no branch predictor, so the leak classes that
dominate the literature — cache-line-granular table lookups, speculation — cannot
manifest. What remains is secret-dependent branching, variable-latency
instructions (`UDIV`/`SDIV`, 2–12 cycles), and early-return comparison. With
interrupts masked, a cycle count becomes a *deterministic function of the input*:
the same input always yields exactly the same number. That is what makes
zero-spread and sub-cycle sensitivity claims possible here and not on a laptop.

## Two experiments, because one would overstate the result

| experiment | varies | measures |
|---|---|---|
| `verify` | the tag, key fixed | the crate's **comparison path** |
| `keyed` | the key (classic dudect) | the crate's **arithmetic core** — AES key schedule and S-box, GHASH, Poly1305, the permutation, scalar multiplication |

They are reported as separate matrices. A clean verdict on one says nothing
about the other.

## Controls, validated per configuration

A null result means nothing without a control that demonstrably leaks — and a
control validated once is not enough. The realistic control (an early-exit tag
comparison) **stops leaking at `-O2`** when LLVM rewrites it branchless, so
`CANARY-control` spends secret-dependent cycles behind `black_box`, which the
compiler may not optimize through. `analysis/check_results.py` enforces in CI
that **every** column has a control that leaks.

## Layout

| path | what |
|---|---|
| `probes` | the registry — every implementation under one interface |
| `rustguard-core` | in-house ASCON-128, `no_std`, Kani-verified, KAT-checked |
| `rustguard-pap` | reboot-robust packet authentication protocol |
| `firmware-tm4c` | TM4C123 harness: perf, timing, clock axis, ladder, public-key |
| `firmware-stm32-timing` | the same harness on ST silicon (STM32F303) |
| `pi-runner` | application-class harness (aarch64 Linux, Raspberry Pi 3/4/5) |
| `capture` | `collect_timing.py` (boards), `import_pi.py` (Pi) |
| `analysis` | `matrix`, `ladder`, `resolution`, `static_vs_measured`, `ct_binary`, `dudect`, `check_results` |
| `paper` | the draft |
| `results` | every raw capture, figure and table |

## Reproducing it

Host only, no hardware:

```sh
cargo test -p rustguard-core -p rustguard-pap   # KATs + protocol
pip install -r analysis/requirements.txt
python -m pytest analysis/tests                 # 27 unit tests
python analysis/selftest.py                     # whole pipeline on synthetic data
python analysis/check_results.py                # invariants of the committed results
```

With a board (TM4C123 or STM32F3 Discovery):

```powershell
scripts\sweep.ps1 -Port COM20 -Board tm4c       # build, flash, capture, build matrix
python analysis\make_figures.py                 # 8 figures + 10 tables
```

On a Raspberry Pi:

```sh
scripts/run_pi.sh 100000                        # then import_pi.py on the analysis host
```

See [`docs/experiment_runbook.md`](docs/experiment_runbook.md) for the full
workflow and [`docs/hardware_setup.md`](docs/hardware_setup.md) for wiring.

## Verification beyond measurement

- **Machine-checked (Kani).** 6/6 proofs: the AEAD and hash are free of panics,
  overflow and UB for all symbolic inputs, and decryption recovers the
  plaintext. Runs in CI. See [`docs/verification.md`](docs/verification.md).
- **Correctness.** Byte-for-byte against the published ASCON-128 reference for 8
  known-answer vectors — real KATs, not round-trip self-consistency.
- **Memory safety.** `rustguard-core` and `rustguard-pap` are `#![no_std]`
  `#![forbid(unsafe_code)]`; the only `unsafe` is isolated MMIO in firmware.

## Honest limitations

- This measures **timing**, not power or EM. Constant-time code can still leak
  through those channels; saying otherwise needs a capture rig.
- Conclusions are stated for **in-order, cacheless Cortex-M4** cores. The leak
  classes excluded by that architecture are exactly the ones that dominate on
  application processors. The `pi-runner` harness extends the study there and is
  pending hardware.
- Zero spread is observed over the **sampled** input space — a strong empirical
  statement, not a proof. It complements static verification rather than
  replacing it.
- The binary census is a **screening heuristic, not a detector**; the false
  positive rate above is the point.

## License

MIT — see [LICENSE](LICENSE).
