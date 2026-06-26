# Experiment runbook

End-to-end workflow from a clean checkout to the figures the paper needs. Steps
marked [HOST] run anywhere; [BENCH] need the hardware.

## 0. Host sanity [HOST]

```sh
cargo test -p rustguard-core -p rustguard-pap
```
Both crates green = the cipher matches the ASCON spec (KATs) and the revised
protocol behaves (reboot/replay/tamper). Do this before anything else.

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

## 2. Side-channel: TVLA on STM32F3 [BENCH]

Validate the chain with the leaky control FIRST, then test the real DUT.

```sh
cd firmware-stm32-tvla
# (a) leaky control
cargo build --release --features leaky && flash
python ../capture/rustguard_capture/capture_tvla.py --variant leaky \
       --out ../results/traces/leaky.npz --n-traces 10000
# (b) constant-time DUT
cargo build --release && flash
python ../capture/rustguard_capture/capture_tvla.py --variant safe \
       --out ../results/traces/safe.npz --n-traces 10000
# (c) analyze both
python ../analysis/tvla.py ../results/traces/safe.npz \
       --leaky-npz ../results/traces/leaky.npz \
       --plot ../results/figures/tvla.png
```

Interpretation:
- leaky control |t| > 4.5 → chain validated.
- safe DUT below 4.5 → constant-time guarantee held through compilation (the
  positive headline result).
- safe DUT above 4.5 somewhere → you found where Rust's CT guarantee breaks on
  silicon (an even more interesting result — investigate which operation/sample).

## 3. Optimization-level sweep (the compiler-betrayal angle) [BENCH]

Rebuild the safe DUT at `opt-level = 0, 1, 2, 3` and `-Z` CT-relevant flags,
capture each, and compare peak |t|. The thesis is that source-level CT can be
preserved or destroyed depending on the optimizer. This sweep is the core
novelty — keep the trace counts equal across builds for a fair comparison.

## 4. Figures the paper needs

- F1: cycles/byte, Rust vs C vs asm, across payload sizes (from step 1).
- F2: TVLA t-trace, leaky control vs constant-time DUT (from step 2).
- F3: peak |t| vs optimization level (from step 3).
- F4: (optional) the reboot/nonce-reuse illustration for the protocol section.

Steps 1–3 produce real artifacts only on the bench. Nothing in this repo
fabricates them; that is by design.
