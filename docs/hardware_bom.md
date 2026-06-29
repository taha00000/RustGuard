# Hardware bill of materials — what to buy, and the plug-and-go flow

The repository is complete in software. Two physical setups produce the two
results: **performance** (cycle counts on a TM4C) and **side-channel TVLA**
(power leakage on an STM32F3). You can buy them independently.

> Before buying anything, run `python analysis/selftest.py`. It exercises the
> entire parse → plot → TVLA chain on synthetic data and writes watermarked
> figures to `results/_demo/`. If that passes (it should, out of the box), the
> only thing standing between you and real figures is the capture hardware.

---

## Setup A — performance / cycle counts (cheap; you may already have it)

| Item | Example part | ~Price | Notes |
|---|---|---|---|
| Cortex-M4F dev board | **EK-TM4C123GXL LaunchPad** | $15–20 | The perf DUT. You already have this. |
| USB–UART dongle | **FT232RL** or **CP2102** | $5–10 | **Required.** Reads cycle counts off PA1. Bypasses the ICDI COM-port driver bug that blocked the previous attempt. |
| Jumper wires | F–F dupont | $3 | dongle RX←PA1, GND←GND |
| Flasher | `lm4flash` (open-source) | free | Avoids LM Flash Programmer / ICDI entirely. |

Wiring: dongle **RX ← LaunchPad PA1** (UART0 TX), dongle **GND ← GND**. Open the
dongle COM port at **115200 8N1**.

## Setup B — side-channel TVLA (the headline result)

Two ways to buy this. **Option B1 is the simplest plug-and-go** because the
STM32F303 target is on the same board as the capture hardware and is already
wired to the standard trigger/UART pins our firmware uses.

### Option B1 — integrated (recommended)
| Item | Example part | ~Price | Notes |
|---|---|---|---|
| Capture + target, one board | **ChipWhisperer-Lite (32-bit / ARM edition)** | $250–300 | On-board **STM32F303** target — exactly `firmware-stm32-tvla`'s target. The FPGA clocks the target (synchronous capture). |

That single board is enough for the TVLA story. Nothing else to wire.

### Option B2 — modular (more flexible, more parts)
| Item | Example part | ~Price | Notes |
|---|---|---|---|
| Capture | **ChipWhisperer-Lite** or **CW-Husky** | $250 / $500 | Husky has more samples/depth. |
| Baseboard | **CW308 UFO** | $50 | Hosts swappable targets. |
| Target | **CW308T-STM32F3** | $30 | STM32F303RCT, same part as B1. |
| SMA cable | included | — | measurement → capture |

Either option gives the standard wiring our firmware assumes: **USART2 on
PA2/PA3 (AF7)**, **trigger on PA12 → CW trigger (tio4)**, **38400 8N1**.

---

## One board-specific thing to know (clock)

`firmware-stm32-tvla` boots on the internal **HSI (8 MHz)** so UART, the trigger,
and the protocol come up the instant you flash — no external-clock dependency.
That is the right setting for **first bring-up** and is enough to validate the
leaky control.

For the *cleanest* first-order TVLA you want the target clocked by the
ChipWhisperer (synchronous `clkgen_x4` capture, which the capture script already
selects). On both B1 and B2 the CW can drive the target clock; when you switch to
it, set `CLOCK_HZ` in `firmware-stm32-tvla/src/main.rs` to the CW clkgen
frequency (commonly 7_370_000) and rebuild. This is a one-line change, clearly
marked in the firmware.

---

## Plug-and-go, once hardware is in hand

```sh
# 0. sanity (no hardware): pipeline is healthy
python analysis/selftest.py

# A. performance (TM4C + USB-UART dongle)
cd firmware-tm4c && cargo build --release
arm-none-eabi-objcopy -O binary target/thumbv7em-none-eabihf/release/firmware-tm4c fw.bin
lm4flash fw.bin
#   capture the dongle output to dump_rust.txt, then:
python analysis/parse_perf.py dump_rust.txt --out results/perf_rust.csv

# B. TVLA (ChipWhisperer + STM32F3) — leaky control FIRST, then the DUT
cd firmware-stm32-tvla
cargo build --release --features leaky && <flash>   # positive control
python capture/rustguard_capture/capture_tvla.py --variant leaky --out results/traces/leaky.npz
cargo build --release && <flash>                     # constant-time DUT
python capture/rustguard_capture/capture_tvla.py --variant safe  --out results/traces/safe.npz

# C. build every figure from whatever you captured
python analysis/make_figures.py        # -> results/figures/{perf,tvla}.png
```

`make_figures.py` builds whichever figures it has inputs for and skips the rest,
so you can run it after Setup A alone, Setup B alone, or both. Flashing commands
(`<flash>`) for the ChipWhisperer are in `docs/hardware_setup.md` and the CW docs.
