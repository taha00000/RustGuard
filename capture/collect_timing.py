#!/usr/bin/env python3
"""Collect dudect-style timing samples from the TM4C timing harness over UART.

Drives the board flashed with `firmware-tm4c --features timing` (and, for the
positive control, `--features "timing leaky"`). For each trace it sends a
fixed-or-random input and records the cycle count the firmware measures with its
DWT counter and interrupts disabled. Output is an .npz consumed by
analysis/dudect.py.

REQUIRES THE BOARD (and a USB-UART dongle on PA1). It is a real serial driver,
not a simulation. To exercise the analysis without hardware, use
analysis/selftest.py instead.

  python capture/collect_timing.py --port COM5 --experiment tagcompare \
         --variant leaky --n 20000 --out results/timing/leaky.npz

Experiments
  encrypt     : class = key+plaintext. Tests the constant-time encrypt path.
  tagcompare  : class = the tag handed to decrypt (correct vs random). Directly
                targets the tag-comparison branch the leaky control changes.
"""
from __future__ import annotations

import argparse
import os
import sys

import numpy as np

try:
    import serial  # pyserial
except ImportError:
    serial = None

FIXED_KEY = bytes.fromhex("00112233445566778899aabbccddeeff")
FIXED_PT = bytes(16)


def _readline(ser) -> str:
    return ser.readline().decode("ascii", "replace").strip()


def _wait_ready(ser):
    # Firmware prints a banner ending in READY after reset.
    for _ in range(50):
        line = _readline(ser)
        if line.endswith("READY"):
            return
    print("warning: did not see READY banner; continuing anyway", file=sys.stderr)


def _cyc(ser, cmd: bytes, *blocks: bytes) -> int:
    payload = cmd + b"".join(b.hex().encode() for b in blocks)
    ser.write(payload)
    line = _readline(ser)
    if not line.startswith("cyc "):
        raise RuntimeError(f"expected 'cyc <n>', got {line!r}")
    return int(line.split()[1])


def _get_correct_tag(ser, key: bytes) -> bytes:
    ser.write(b"g" + key.hex().encode())
    line = _readline(ser)
    if not line.startswith("tag "):
        raise RuntimeError(f"expected 'tag <hex>', got {line!r}")
    return bytes.fromhex(line.split()[1])


def collect(cfg):
    if serial is None:
        sys.exit("pyserial not installed. `pip install pyserial`.")
    ser = serial.Serial(cfg.port, cfg.baud, timeout=2)
    _wait_ready(ser)
    rng = np.random.default_rng(cfg.seed)

    correct_tag = _get_correct_tag(ser, FIXED_KEY) if cfg.experiment == "tagcompare" else None

    cycles = np.empty(cfg.n, dtype=np.uint32)
    labels = np.empty(cfg.n, dtype=np.uint8)
    for i in range(cfg.n):
        is_random = (i % 2) == 1  # interleave fixed/random to reject drift
        if cfg.experiment == "encrypt":
            if is_random:
                key = bytes(rng.integers(0, 256, 16, dtype=np.uint8))
                pt = bytes(rng.integers(0, 256, 16, dtype=np.uint8))
            else:
                key, pt = FIXED_KEY, FIXED_PT
            c = _cyc(ser, b"e", key, pt)
        else:  # tagcompare
            tag = bytes(rng.integers(0, 256, 16, dtype=np.uint8)) if is_random else correct_tag
            c = _cyc(ser, b"v", FIXED_KEY, tag)
        cycles[i] = c
        labels[i] = 1 if is_random else 0
        if (i + 1) % 2000 == 0:
            print(f"  {i + 1}/{cfg.n}", flush=True)

    ser.close()
    os.makedirs(os.path.dirname(cfg.out) or ".", exist_ok=True)
    np.savez_compressed(cfg.out, cycles=cycles, labels=labels,
                        variant=cfg.variant, experiment=cfg.experiment)
    print(f"saved {cfg.n} samples -> {cfg.out}  "
          f"(fixed mean={cycles[labels==0].mean():.1f}c, "
          f"random mean={cycles[labels==1].mean():.1f}c)")


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", required=True, help="serial port, e.g. COM5 or /dev/ttyUSB0")
    ap.add_argument("--baud", type=int, default=115200)
    ap.add_argument("--experiment", choices=["encrypt", "tagcompare"], default="tagcompare")
    ap.add_argument("--variant", choices=["safe", "leaky"], default="safe",
                    help="which firmware is flashed (metadata label only)")
    ap.add_argument("--n", type=int, default=20000)
    ap.add_argument("--seed", type=int, default=0xC0FFEE)
    ap.add_argument("--out", default="results/timing/safe.npz")
    collect(ap.parse_args())


if __name__ == "__main__":
    main()
