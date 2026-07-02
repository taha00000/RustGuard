# Flashing the TM4C123 (Windows + LM Flash Programmer)

One command builds and flashes; a USB-UART dongle carries the data. Two things
plug into the laptop:

1. **The LaunchPad** via its **DEBUG (ICDI) micro-USB** port — power + flashing.
2. **A USB-UART dongle** (FT232RL / CP2102) — the measurement data, wired to the
   board: **dongle RX ← PA1**, **dongle TX → PA0**, **dongle GND ← GND**, at
   **115200 8N1**. This is deliberate: the board's ICDI virtual COM port has a
   Windows driver conflict (it blocked capture in the first attempt); the dongle
   bypasses it. See `docs/hardware_setup.md`.

## Prerequisites (all confirmed present on this laptop except the board)

- `rustup target add thumbv7em-none-eabihf` — installed
- `cargo objcopy` (cargo-binutils) — installed (builds the `.bin`)
- LM Flash Programmer — installed at
  `C:\Program Files (x86)\Texas Instruments\Stellaris\LM Flash Programmer\LMFlash.exe`
- A USB-UART dongle + 3 jumper wires — **the one thing to buy (~$8)**

## Flash (one command each)

```powershell
scripts\flash.ps1 -Mode perf           # performance benchmark
scripts\flash.ps1 -Mode timing         # dudect timing harness (constant-time DUT)
scripts\flash.ps1 -Mode timing-leaky   # timing harness with the leaky control
scripts\flash.ps1 -Mode timing -BuildOnly   # build+objcopy only, no board
```

The script runs `cargo objcopy` then
`LMFlash.exe -q manual -i ICDI -e all -v -r fw.bin` (erase, verify, reset). If the
CLI ever misbehaves, the GUI is the always-works fallback: open LM Flash
Programmer → **Configuration** tab → pick **EK-TM4C123GXL** → **Program** tab →
select `firmware-tm4c\fw.bin` → **Program**.

## First-boot sanity check (2 minutes — do this once)

After flashing a **timing** build, open the dongle's COM port at 115200 8N1
(PuTTY / `python -m serial.tools.miniterm COM<N> 115200`). You should see:

```
# RustGuard TM4C123 timing harness (safe)
READY
```
Type `k` followed by 32 hex chars (a key); the board replies `z`. If you get the
banner and the `z` ack, the firmware's clock/UART/DWT setup is correct and every
downstream step will work. If nothing prints, re-check the dongle wiring (RX/TX
not swapped) before capturing.

## Full flow

```powershell
# performance
scripts\flash.ps1 -Mode perf
#   capture dongle output -> dump_rust.txt
python analysis\parse_perf.py dump_rust.txt --out results\perf_rust.csv

# timing (leaky control FIRST, then the safe DUT)
scripts\flash.ps1 -Mode timing-leaky
python capture\collect_timing.py --port COM5 --variant leaky --out results\timing\leaky.npz
scripts\flash.ps1 -Mode timing
python capture\collect_timing.py --port COM5 --variant safe  --out results\timing\safe.npz

# build every figure + table from whatever you captured
python analysis\make_figures.py
```

The proof, source, and binary legs (`cargo kani`, `cargo test`, `ct_binary.py`)
need no hardware and already produce real results.
