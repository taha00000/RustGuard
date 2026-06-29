# Hardware bill of materials — what to buy, and the plug-and-go flow

The entire study runs on **Cortex-M4 dev boards** — no oscilloscope, no
ChipWhisperer, no lab equipment. Both results (performance and timing leakage) use
the board's own DWT cycle counter as the instrument.

> Before buying anything, run `python analysis/selftest.py`. It exercises the
> entire parse → plot → timing-leakage chain on synthetic data and writes
> watermarked figures to `results/_demo/`. If that passes (it should, out of the
> box), the only thing between you and real figures is flashing a board.

---

## Minimum (you likely already have most of this)

| Item | Example part | ~Price | Notes |
|---|---|---|---|
| Cortex-M4F dev board | **EK-TM4C123GXL LaunchPad** | $15–20 | The DUT for both perf and timing. You have this. |
| USB–UART dongle | **FT232RL** or **CP2102** | $5–10 | **Required.** RX←PA1, TX→PA0, GND←GND. Bypasses the ICDI driver bug. |
| Jumper wires | F–F dupont | $3 | three wires |
| Flasher | `lm4flash` (open-source) | free | Avoids LM Flash Programmer / ICDI entirely. |
| arm-none-eabi toolchain | `objcopy` for `.bin` | free | + `rustup target add thumbv7em-none-eabihf` |

That is enough for the complete paper: performance (Rust vs C vs asm) **and** the
dudect timing-leakage result with its leaky-vs-safe control and optimization sweep.

## Recommended (strengthens the paper): 1–2 more M4 boards

A cross-silicon result — the same timing finding on different-vendor Cortex-M4
microarchitectures — is a real robustness argument and cheap to add. Any of:

| Item | ~Price | Why |
|---|---|---|
| **STM32F4 "Black Pill" (STM32F411)** | $6–10 | Different vendor/microarchitecture, M4, trivial to source |
| **nRF52840 dev board / dongle** | $10–30 | Third vendor (Nordic), M4 |
| A second **EK-TM4C123GXL** | $15–20 | Same-part reproducibility / spare |

Porting the timing harness to these is a small job: **only `uart_init`/`getc`/
`putc` are board-specific** (DWT and the crypto are identical across M4). The
performance numbers can stay on the TM4C either way.

## Explicitly NOT needed

- **No ChipWhisperer / CW308 / STM32F3 target.** Power/EM analysis is future work.
- **No oscilloscope or shunt resistor.** Timing is read from the DWT counter.

---

## Plug-and-go, once the board is flashed

```sh
# 0. sanity (no hardware): pipeline is healthy
python analysis/selftest.py

# A. performance
cd firmware-tm4c && cargo build --release
arm-none-eabi-objcopy -O binary target/thumbv7em-none-eabihf/release/firmware-tm4c fw.bin
lm4flash fw.bin
#   capture the dongle output to dump_rust.txt, then:
python ../analysis/parse_perf.py dump_rust.txt --out ../results/perf_rust.csv

# B. timing leakage — leaky control FIRST, then the DUT
cargo build --release --features "timing leaky" && lm4flash fw.bin   # control
python ../capture/collect_timing.py --port COM5 --variant leaky --out ../results/timing/leaky.npz
cargo build --release --features timing && lm4flash fw.bin           # DUT
python ../capture/collect_timing.py --port COM5 --variant safe  --out ../results/timing/safe.npz

# C. build every figure from whatever you captured
python ../analysis/make_figures.py        # -> results/figures/{perf,timing}.png
```

`make_figures.py` builds whichever figures it has inputs for and skips the rest,
so it works after the perf step alone, the timing step alone, or both. Set your
actual serial port in place of `COM5`. Full detail in `docs/experiment_runbook.md`
and `docs/hardware_setup.md`.
