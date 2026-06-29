# Experiment runbook

End-to-end workflow from a clean checkout to the figures the paper needs. Steps
marked [HOST] run anywhere; [BENCH] need the TM4C board (+ a USB-UART dongle).
**No oscilloscope or ChipWhisperer is required** — the side-channel result is a
timing measurement taken with the chip's own cycle counter.

## 0. Host sanity [HOST]

```sh
cargo test -p rustguard-core -p rustguard-pap
```
Both crates green = the cipher matches the ASCON spec (KATs) and the revised
protocol behaves (reboot/replay/tamper). Do this before anything else.

Then dry-run the figure pipeline on synthetic data (still no hardware):

```sh
pip install -r analysis/requirements.txt
python analysis/selftest.py
```
This confirms the perf parser, the perf figure, and the dudect timing t-test all
work end-to-end. It writes **watermarked, synthetic** figures to `results/_demo/`
(gitignored) — a pipeline check, never a result. See `docs/hardware_bom.md` for
exactly what to buy.

## 1. Performance: Rust vs C/asm on TM4C [BENCH]

```sh
# Rust
cd firmware-tm4c && cargo build --release
arm-none-eabi-objcopy -O binary target/thumbv7em-none-eabihf/release/firmware-tm4c fw.bin
lm4flash fw.bin
# capture PA1 -> USB-UART dongle @115200, save to dump_rust.txt, then:
python ../analysis/parse_perf.py dump_rust.txt --out ../results/perf_rust.csv

# C / asm baselines
cd ../baseline-c && ./setup.sh && make tm4c-bench
# flash + capture each -> dump_cref.txt, dump_pqm4.txt, parse the same way
```
Deliverable: `results/perf_*.csv` → the cycles/byte and "cost of memory safety"
comparison figure.

## 2. Timing leakage: dudect on TM4C [BENCH]

The side-channel result, on the same board. Validate the method with the leaky
control FIRST, then test the real DUT.

```sh
cd firmware-tm4c
# (a) leaky control — known-leaking variable-time tag compare
cargo build --release --features "timing leaky" && <flash fw.bin>
python ../capture/collect_timing.py --port COM5 --experiment tagcompare \
       --variant leaky --n 20000 --out ../results/timing/leaky.npz
# (b) constant-time DUT
cargo build --release --features timing && <flash fw.bin>
python ../capture/collect_timing.py --port COM5 --experiment tagcompare \
       --variant safe --n 20000 --out ../results/timing/safe.npz
# (c) analyze both together
python ../analysis/dudect.py ../results/timing/safe.npz \
       --leaky ../results/timing/leaky.npz \
       --plot ../results/figures/timing.png
```

Interpretation:
- leaky control |t| > 4.5 → the method and harness are validated.
- safe DUT below 4.5 → the constant-time guarantee held through compilation (the
  positive headline result).
- safe DUT above 4.5 → you found where Rust's constant-time guarantee breaks on
  silicon (the more interesting result — note the optimization level and dig in).

Tips for clean timing samples: the harness already disables interrupts and uses
`black_box` around the measured op. Take ≥20k traces; interleaving fixed/random
(the collector does this) rejects slow drift. Also run `--experiment encrypt` to
check the encrypt path, not just the tag compare.

## 3. Optimization-level sweep (the compiler-betrayal angle) [BENCH]

Rebuild the constant-time timing firmware at `opt-level = 0,1,2,3` (set in
`firmware-tm4c/Cargo.toml` or via `RUSTFLAGS`), collect each to its own
`safe_O{n}.npz`, and compare peak |t|. The thesis is that source-level
constant-time can be preserved or destroyed by the optimizer. Keep the trace
count equal across builds for a fair comparison. This sweep is the core novelty.

## 4. Cross-silicon (optional, strengthens the paper) [BENCH]

Port the timing harness to a second Cortex-M4 (e.g. an STM32F4 "Black Pill" or an
nRF52840) — only the UART init changes; DWT is identical across M4 parts. Re-run
step 2 on each board to show the finding is not microarchitecture-specific.

## 5. Build the figures and tables

One command builds every artifact for which inputs exist, skipping the rest:

```sh
# optional: code-size table input
arm-none-eabi-size firmware-*/target/thumbv7em-none-eabihf/release/firmware-* \
    > results/size.txt
python analysis/make_figures.py     # -> results/figures/*.png + results/tables/*.{md,tex}
```

Figures (`results/figures/`):
- `perf_throughput.png` — cycles/byte vs size, Rust vs C vs asm (step 1)
- `perf_overhead.png`   — Rust-over-baseline overhead, the cost of memory safety
- `perf_perm.png`       — p6/p12 permutation cost per implementation
- `timing.png`          — dudect histograms, leaky control vs constant-time DUT (step 2)
- `timing_convergence.png` — |t| vs number of traces (statistical rigor)
- `opt_sweep.png`       — peak |t| vs optimization level (step 3, the headline)

Tables (`results/tables/`, Markdown + LaTeX `\input`-ready):
- `perf_cycles` — per-size cycle counts + overhead
- `timing`      — peak |t|, verdict, class means per capture
- `codesize`    — flash/RAM per build
- `opt_sweep`   — peak |t| per optimization level

For the opt-sweep figure/table, capture one `results/timing/safe_O{n}.npz` per
optimization level (rebuild firmware at `-O0..-O3` via `RUSTFLAGS`/Cargo profile).

Everything here is produced only from real bench artifacts. The synthetic
`results/_demo/` output of `selftest.py` is watermarked and never enters
`results/figures/` or `results/tables/`. Power/EM side-channel analysis (needs a
ChipWhisperer-class rig) is out of scope and left as future work.
