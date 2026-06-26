#!/usr/bin/env python3
"""Fixed-vs-random TVLA capture driver for RustGuard on the CW308/STM32F3.

Drives the ChipWhisperer to collect two interleaved trace classes per the
ISO/IEC 17825 / TVLA methodology (Goodwill et al. 2011):

  * fixed class  : key (or input) held at a constant value
  * random class : key (or input) drawn uniformly at random each trace

Interleaving fixed/random reduces the chance that slow environmental drift
(temperature, supply) is mistaken for data-dependent leakage.

Output: an .npz with traces, class labels, and metadata, consumed by
analysis/tvla.py.

REQUIRES PHYSICAL HARDWARE. This script will not produce results without a
connected ChipWhisperer and a flashed STM32F3 target. It is complete and
runnable; it is not a simulation.
"""
from __future__ import annotations

import argparse
import os
import sys
import time

import numpy as np

try:
    import chipwhisperer as cw
except ImportError:
    cw = None


FIXED_KEY = bytes.fromhex("00112233445566778899aabbccddeeff")


def connect(cfg):
    if cw is None:
        sys.exit(
            "chipwhisperer not installed. `pip install chipwhisperer` and run on "
            "the capture host with the CW connected."
        )
    scope = cw.scope()
    scope.default_setup()
    scope.adc.samples = cfg.samples
    scope.adc.offset = cfg.offset
    scope.clock.adc_src = "clkgen_x4"
    scope.trigger.triggers = "tio4"  # CW308 trigger line
    target = cw.target(scope, cw.targets.SimpleSerial)
    target.baud = cfg.baud
    return scope, target


def one_trace(scope, target, key, plaintext):
    target.simpleserial_write("k", key)
    target.simpleserial_wait_ack()
    scope.arm()
    target.simpleserial_write("p", plaintext)
    if scope.capture():
        return None  # timeout
    target.simpleserial_read("r", 16)  # tag echo
    return scope.get_last_trace()


def capture(cfg):
    scope, target = connect(cfg)
    rng = np.random.default_rng(cfg.seed)

    traces = []
    labels = []  # 0 = fixed, 1 = random
    fixed_pt = bytes(rng.integers(0, 256, 16, dtype=np.uint8))

    t0 = time.time()
    n = 0
    while n < cfg.n_traces:
        is_random = (n % 2) == 1
        if is_random:
            key = bytes(rng.integers(0, 256, 16, dtype=np.uint8))
            pt = bytes(rng.integers(0, 256, 16, dtype=np.uint8))
        else:
            key = FIXED_KEY
            pt = fixed_pt
        tr = one_trace(scope, target, key, pt)
        if tr is None:
            continue  # retry on timeout
        traces.append(tr)
        labels.append(1 if is_random else 0)
        n += 1
        if n % 500 == 0:
            rate = n / (time.time() - t0)
            print(f"  {n}/{cfg.n_traces}  ({rate:.0f} tr/s)", flush=True)

    scope.dis()
    target.dis()

    traces = np.asarray(traces, dtype=np.float32)
    labels = np.asarray(labels, dtype=np.uint8)
    os.makedirs(os.path.dirname(cfg.out) or ".", exist_ok=True)
    np.savez_compressed(
        cfg.out,
        traces=traces,
        labels=labels,
        samples=cfg.samples,
        target=cfg.target_name,
        variant=cfg.variant,
    )
    print(f"saved {traces.shape[0]} traces x {traces.shape[1]} samples -> {cfg.out}")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--n-traces", type=int, default=10000)
    ap.add_argument("--samples", type=int, default=5000)
    ap.add_argument("--offset", type=int, default=0)
    ap.add_argument("--baud", type=int, default=38400)
    ap.add_argument("--seed", type=int, default=0xC0FFEE)
    ap.add_argument("--out", default="results/traces/capture.npz")
    ap.add_argument(
        "--variant",
        choices=["safe", "leaky"],
        default="safe",
        help="which firmware is flashed: constant-time (safe) or control (leaky)",
    )
    ap.add_argument("--target-name", default="stm32f3-cw308")
    cfg = ap.parse_args()
    capture(cfg)


if __name__ == "__main__":
    main()
