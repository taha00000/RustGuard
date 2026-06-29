#!/usr/bin/env python3
"""End-to-end pipeline self-test on SYNTHETIC data — no hardware required.

This does NOT produce research results. It fabricates obviously-synthetic inputs
and runs them through the exact same parse -> plot and dudect-timing -> figure
code paths the bench uses, so you can confirm the whole chain works before the
hardware arrives. Everything it writes goes to a separate, gitignored directory
(`results/_demo/`) and every figure is stamped with a red SYNTHETIC watermark.

What it checks:
  * the perf parser + perf figure build from a TM4C-style UART dump
  * the dudect timing t-test flags a deliberately leaky control (|t| > 4.5)
  * the dudect timing t-test does NOT flag constant-time-like timing (|t| < 4.5)

When the real hardware is connected you run the *real* scripts instead:
  collect_timing.py -> results/timing/*.npz ; parse_perf.py -> results/*.csv ;
  make_figures.py -> results/figures/*.png  (no watermark, real data).

  python analysis/selftest.py
"""
from __future__ import annotations

import os
import sys

import numpy as np

from parse_perf import parse
from plot_perf import make_perf_figure
from dudect import THRESHOLD, make_dudect_figure
from figutil import ensure_parent, read_perf_csv

WATERMARK = "SYNTHETIC - PIPELINE SELF-TEST - NOT MEASURED DATA"
DEMO_DIR = os.path.join("results", "_demo")
SIZES = [8, 16, 32, 64, 128, 256, 512]


def _fake_perf_dump(base_cpb: float) -> str:
    """A TM4C-firmware-style UART dump. `base_cpb` scales cycles/byte so the
    three synthetic implementations differ (rust > cref > pqm4)."""
    lines = [
        "# RustGuard SYNTHETIC perf dump (self-test, not measured)",
        f"PERM p6 mean_cyc={int(base_cpb * 6)}",
        f"PERM p12 mean_cyc={int(base_cpb * 12)}",
        "SECTION:ENCRYPT",
    ]
    for sz in SIZES:
        # mild fixed-overhead curve so cyc/byte falls with size, like real AEAD
        mean = int(base_cpb * sz + base_cpb * 24)
        lines.append(f"ENC {sz} mean_cyc={mean} cpb_x100={int(mean * 100 / sz)}")
    lines.append("SECTION:DECRYPT")
    for sz in SIZES:
        mean = int(base_cpb * sz + base_cpb * 26)
        lines.append(f"DEC {sz} mean_cyc={mean}")
    lines.append("SECTION:DONE")
    return "\n".join(lines) + "\n"


def _synth_timing(rng, n: int, leak: bool):
    """Per-class cycle counts, like the TM4C timing harness would report. A small
    measurement jitter is added so the t-test path is exercised normally. If
    leak=True the random class returns earlier and with more spread (a stand-in
    for the variable-time tag compare); otherwise both classes share one
    distribution (constant-time)."""
    labels = np.tile([0, 1], n // 2).astype(np.uint8)
    cycles = rng.normal(4200.0, 2.0, size=n)  # constant-time baseline + tiny jitter
    if leak:
        # random class: early return -> fewer cycles, position-dependent spread
        rand_mask = labels == 1
        cycles[rand_mask] = rng.normal(4180.0, 8.0, size=int(rand_mask.sum()))
    return np.rint(cycles).astype(np.uint32), labels


def _save_timing(path, cycles, labels, variant, experiment):
    ensure_parent(path)
    np.savez_compressed(path, cycles=cycles, labels=labels,
                        variant=variant, experiment=experiment)


def main() -> int:
    print(f"=== RustGuard pipeline self-test (SYNTHETIC) -> {DEMO_DIR}/ ===")
    rng = np.random.default_rng(20260626)
    fig_dir = os.path.join(DEMO_DIR, "figures")
    raw_dir = os.path.join(DEMO_DIR, "raw")
    os.makedirs(fig_dir, exist_ok=True)
    os.makedirs(raw_dir, exist_ok=True)
    ok = True

    # 1) perf: dump -> parse_perf.parse -> CSV -> read back -> figure
    datasets = {}
    for name, base in (("rust", 8.0), ("cref", 6.5), ("pqm4", 4.0)):
        dump_path = os.path.join(raw_dir, f"perf_{name}.txt")
        with open(dump_path, "w") as f:
            f.write(_fake_perf_dump(base))
        with open(dump_path) as f:
            rows, _perm = parse(f)
        csv_path = os.path.join(DEMO_DIR, f"perf_{name}.csv")
        ensure_parent(csv_path)
        import csv as _csv
        with open(csv_path, "w", newline="") as f:
            w = _csv.DictWriter(f, fieldnames=["op", "size", "mean_cyc", "cyc_per_byte"])
            w.writeheader()
            w.writerows(rows)
        datasets[name] = read_perf_csv(csv_path)
        if not rows:
            print(f"  [perf] FAIL: parser produced no rows for {name}")
            ok = False
    perf_fig = make_perf_figure(datasets, os.path.join(fig_dir, "perf.png"), WATERMARK)
    print(f"  [perf] built {perf_fig} (3 synthetic implementations)")

    # 2) dudect timing: synth safe + leaky -> figure, and assert the verdicts
    safe_c, safe_l = _synth_timing(rng, 20000, leak=False)
    leaky_c, leaky_l = _synth_timing(rng, 20000, leak=True)
    safe_npz = os.path.join(DEMO_DIR, "timing", "safe.npz")
    leaky_npz = os.path.join(DEMO_DIR, "timing", "leaky.npz")
    _save_timing(safe_npz, safe_c, safe_l, "safe-synth", "tagcompare")
    _save_timing(leaky_npz, leaky_c, leaky_l, "leaky-synth", "tagcompare")
    results = make_dudect_figure(safe_npz, leaky_npz,
                                 os.path.join(fig_dir, "timing.png"), WATERMARK)
    by = {r["name"].split()[0]: r for r in results}
    if not by["leaky-control"]["leaking"]:
        print("  [timing] FAIL: synthetic leaky control did not trip the threshold")
        ok = False
    if by["safe"]["leaking"]:
        print("  [timing] FAIL: synthetic constant-time control falsely flagged")
        ok = False
    lt = by["leaky-control"]["t"]
    st = by["safe"]["t"]
    print(f"  [timing] leaky |t|={lt:.1f} (>{THRESHOLD}), safe |t|={st:.1f} (<{THRESHOLD})")

    print("\n" + ("SELF-TEST PASSED" if ok else "SELF-TEST FAILED"))
    print("These figures are SYNTHETIC and watermarked. Real results come from "
          "the bench via make_figures.py.")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
