# Hardware setup

Two targets. The performance story runs on the TM4C123; the side-channel (TVLA)
story runs on the STM32F303 on a ChipWhisperer CW308 because its capture path is
clean and natively supported. Keeping them separate is deliberate and strengthens
the paper — the constant-time finding is shown not to be an artifact of one board.

---

## Target A — TM4C123GH6PM (performance / cycle counts)

**Board:** EK-TM4C123GXL LaunchPad. **Clock:** start at 16 MHz internal; the
80 MHz PLL is an optional second data point.

### The UART capture fix (this is what failed before)

Do **not** rely on the ICDI virtual COM port — that is the Windows driver
conflict that blocked capture in the previous attempt. Instead:

1. Wire an external USB-UART dongle (FT232RL or CP2102):
   - dongle **RX** ← LaunchPad **PA1** (UART0 TX)
   - dongle **GND** ← LaunchPad **GND**
   - (TX→PA0 only needed if you later send commands to the board)
2. Open the dongle's COM port at **115200 8N1**.
3. Flash with `cargo build --release` in `firmware-tm4c/`, convert to `.bin`
   with `arm-none-eabi-objcopy`, and load via `lm4flash` (open-source, avoids
   LM Flash Programmer / ICDI entirely).
4. Capture the serial dump to a file, then `python analysis/parse_perf.py dump.txt`.

This produces **real measured** cycle counts — the derived pqm4-scaled numbers
from the old repo are gone and must never come back.

---

## Target B — STM32F303 on CW308 UFO (side-channel / TVLA)

**Why not the TM4C for TVLA?** The TM4C is not a stock ChipWhisperer target;
power capture would need a custom shunt and the alignment quality is worse. The
STM32F3 sits in the CW308 socket with a measurement path designed for this.

### Wiring / trigger

- CW308 provides the measurement shunt and the **tio4** trigger line.
- Firmware raises **PA12 → CW308 GPIO4 (trigger)** immediately before the crypto
  op and lowers it immediately after. **Do not use a UART trigger** — its jitter
  destroys first-order TVLA alignment.
- UART to the target at **38400 8N1** (ChipWhisperer default) for the
  simpleserial-style key/plaintext protocol.

> TODO(hardware): the register addresses and `clocks_uart_init()` in
> `firmware-stm32-tvla/src/main.rs` are marked and must be confirmed against the
> exact STM32F303 part on your CW308 before the first capture. The trigger pin
> (PA12) is the documented default — verify continuity to tio4 with a meter.

### Two builds, always

```sh
# 1. positive control — known-leaking, validates the whole chain
cargo build --release --features leaky    # -> leaky binary
#    flash it, then:
python capture/rustguard_capture/capture_tvla.py --variant leaky \
       --out results/traces/leaky.npz

# 2. device under test — the constant-time implementation
cargo build --release                     # -> safe binary
#    flash it, then:
python capture/rustguard_capture/capture_tvla.py --variant safe \
       --out results/traces/safe.npz

# 3. analyze both together
python analysis/tvla.py results/traces/safe.npz \
       --leaky-npz results/traces/leaky.npz
```

If the leaky control does not exceed |t| = 4.5, the rig is broken and the safe
result is meaningless. The paper reports both, in that order.

---

## Epoch persistence (for the protocol claim)

The reboot-robustness claim depends on durably committing the boot epoch. On the
TM4C use the on-chip EEPROM (2 KB) with a two-slot A/B record and a validity
flag, so a write torn by brown-out is detected and the last good epoch is used.
Wiring this into `firmware-tm4c` is a TODO; the protocol crate exposes the
`EpochStore` trait for exactly this.
