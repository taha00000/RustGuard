# Cross-silicon: the same constant-time result on a second vendor's M4

The performance and timing results run on the TI TM4C123. Repeating the timing
experiment on the **STM32F303** (STM32F3 Discovery) — a Cortex-M4 from a different
vendor with a different microarchitecture — shows the finding is not an artifact
of one chip. `firmware-stm32-timing` is a straight port of the TM4C timing harness
(same protocol, same DWT measurement); only the UART/clock bring-up differs, so
the same `collect_timing.py` and `dudect.py` drive it.

## What you need (three one-time items)

1. **STM32CubeProgrammer** (free from ST) — the flashing tool. Install it; the
   CLI lands at
   `C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe`.
   (Installing it also installs the ST-LINK USB driver.)
2. **The STM32F3 Discovery board**, plugged into the laptop by its **ST-LINK**
   USB (the mini-USB at the top edge). This powers + flashes the board.
3. **A USB-UART dongle + 3 jumper wires** — because the F3 Discovery's ST-LINK has
   **no serial port**, the measurement data comes over the dongle:

   | dongle pin | STM32 Discovery pin |
   |---|---|
   | **RX** | **PA2** (USART2 TX) |
   | **TX** | **PA3** (USART2 RX) |
   | **GND** | any **GND** |

   PA2/PA3 are on the Discovery's 2-row headers. Plug the dongle into a second
   USB port; it appears as its own COM port at **115200 8N1**.

## Run it

```powershell
# leaky control first, then the constant-time DUT
scripts\flash_stm32.ps1 -Mode leaky
python capture\collect_timing.py --port COM<N> --variant leaky --out results\timing\stm32_leaky.npz
scripts\flash_stm32.ps1 -Mode safe
python capture\collect_timing.py --port COM<N> --variant safe  --out results\timing\stm32_safe.npz

# analyze (same tool as the TM4C)
python analysis\dudect.py results\timing\stm32_safe.npz --leaky results\timing\stm32_leaky.npz `
       --plot results\figures\timing_stm32.png
```

Use the dongle's COM number for `COM<N>` (Device Manager → Ports).

## First-boot sanity check

After flashing a build, open the dongle COM port at 115200 and confirm a `READY`
banner and that a `g`<32 hex> command replies with `tag <hex>`. That proves the
STM32 UART/clock bring-up is correct before trusting a capture. (These register
values target the STM32F303 per RM0316; the first bring-up is the one place to
verify them on the actual board.)

## Expected result

The constant-time DUT should again show `|t|` well under 4.5 (matching the TM4C's
|t| = 0.00), and the leaky control should trip hard — demonstrating the
constant-time property holds on a second, independent Cortex-M4.
