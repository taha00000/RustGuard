# Hardware setup

Everything runs on a **single Cortex-M4** (the TM4C123 you already have) plus a
USB-UART dongle. No oscilloscope, no ChipWhisperer. Both halves of the study —
performance and timing leakage — use the same board and the same UART; only the
firmware build differs.

---

## The board and the UART (read first)

**Board:** EK-TM4C123GXL LaunchPad. **Clock:** 16 MHz internal to start; the
80 MHz PLL is an optional second data point.

### The UART capture fix (this is what failed before)

Do **not** rely on the ICDI virtual COM port — that is the Windows driver conflict
that blocked capture in the previous attempt. Instead:

1. Wire an external USB-UART dongle (FT232RL or CP2102):
   - dongle **RX** ← LaunchPad **PA1** (UART0 TX)
   - dongle **TX** → LaunchPad **PA0** (UART0 RX) — needed for the timing harness,
     which receives commands from the host
   - dongle **GND** ← LaunchPad **GND**
2. Open the dongle's COM port at **115200 8N1**.
3. Flash with `cargo build --release [--features ...]` in `firmware-tm4c/`, convert
   to `.bin` with `arm-none-eabi-objcopy`, and load via `lm4flash` (open-source,
   avoids LM Flash Programmer / ICDI entirely).

This produces **real measured** cycle counts — the derived pqm4-scaled numbers
from the old repo are gone and must never come back.

---

## Mode A — performance (default build)

```sh
cd firmware-tm4c && cargo build --release && <flash>
# capture the UART dump, then:
python ../analysis/parse_perf.py dump.txt --out ../results/perf_rust.csv
```
The firmware streams `PERM/ENC/DEC ... mean_cyc=...` lines and ends with
`SECTION:DONE`. Repeat for the C and pqm4 baselines (see `baseline-c/`).

## Mode B — timing leakage (`--features timing`)

The same board becomes the side-channel instrument. The firmware enters a command
loop; the host (`capture/collect_timing.py`) drives fixed-vs-random inputs and
reads back the DWT cycle count the firmware measures with interrupts disabled.

```sh
# leaky positive control first (validates the method)
cargo build --release --features "timing leaky" && <flash>
python ../capture/collect_timing.py --port COM5 --experiment tagcompare \
       --variant leaky --out ../results/timing/leaky.npz
# constant-time device under test
cargo build --release --features timing && <flash>
python ../capture/collect_timing.py --port COM5 --experiment tagcompare \
       --variant safe --out ../results/timing/safe.npz
# analyze
python ../analysis/dudect.py ../results/timing/safe.npz \
       --leaky ../results/timing/leaky.npz
```

If the leaky control does not exceed |t| = 4.5, the harness or the sample count is
wrong and the safe result is meaningless. The paper reports both, in that order.

Why timing works here: ASCON on Cortex-M4 uses only fixed-cycle bitwise ops, so a
constant-time implementation yields input-independent cycle counts; the variable-
time tag compare returns early on a mismatch, which the cycle counter sees
directly. This is the dudect method (DATE 2017), and it needs no external probe.

---

## Cross-silicon (optional)

Porting the timing harness to another Cortex-M4 (STM32F4 "Black Pill" ~$8,
nRF52840, a second TM4C) is a small change: **only `uart_init` / `getc` / `putc`
are board-specific** — the DWT cycle counter, the crypto, and the protocol are
identical across M4 parts. Running the same experiment on multiple vendors shows
the constant-time finding is not a single-microarchitecture artifact.

---

## Out of scope: power / EM

Power and electromagnetic side channels can leak even from constant-*time* code
and would need a ChipWhisperer-class capture rig. That is deliberately **future
work**; the previous power-capture firmware and ChipWhisperer driver were removed
to keep the repo consistent with what runs on a bare board. They remain in git
history if a capture rig becomes available later.

---

## Epoch persistence (for the protocol claim)

The reboot-robustness claim depends on durably committing the boot epoch. On the
TM4C use the on-chip EEPROM (2 KB) with a two-slot A/B record and a validity flag,
so a write torn by brown-out is detected and the last good epoch is used. Wiring
this into `firmware-tm4c` is a TODO; the protocol crate exposes the `EpochStore`
trait for exactly this.
