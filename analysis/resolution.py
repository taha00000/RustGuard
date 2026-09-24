#!/usr/bin/env python3
"""Measurement resolution and the detection bound behind every clean verdict.

A t-test can only ever say "no leakage was detected", which invites the obvious
reviewer question: *detected at what sensitivity?* On an in-order, cacheless
Cortex-M4 with interrupts masked, the DWT cycle counter is exact and the cycle
count is a deterministic function of the input, so the answer is unusually
strong and worth stating precisely:

  * the counter resolves **one clock cycle** — there is no sampling noise to
    average away, unlike an oscilloscope-based setup;
  * consequently a primitive whose observed cycle counts span **zero** cycles
    across thousands of distinct secret inputs has no input-dependent timing
    *at all* over the sampled space, not merely a difference too small to see;
  * and any leak of a single cycle would have shown up as a non-zero spread.

This module reports, per capture, the observed spread (max - min) within each
class and the number of distinct cycle counts seen. `spread = 0` with
`distinct = 1` is the strongest outcome available from a timing experiment:
every input produced bit-identical timing.

  python analysis/resolution.py --timing-dir results/timing \
         --table results/tables/resolution.md
"""
from __future__ import annotations

import argparse
import glob
import os

import numpy as np

from dudect import load
from figutil import write_table

# Board clock during capture, for turning cycles into wall-clock sensitivity.
CLOCK_HZ = {"tm4c": 16_000_000, "stm32": 8_000_000}


def summarize(timing_dir: str, experiment: str = "verify"):
    """-> rows of (board, opt, primitive, n, spread_fixed, spread_rand, distinct)."""
    rows = []
    for path in sorted(glob.glob(os.path.join(timing_dir, "*.npz"))):
        d = np.load(path)
        if not ("board" in d and "opt" in d and "probe" in d):
            continue
        if str(d.get("experiment", "verify")) != experiment:
            continue
        cyc, lab, _v, _e = load(path)
        f, r = cyc[lab == 0], cyc[lab == 1]
        rows.append(
            {
                "board": str(d["board"]),
                "opt": str(d["opt"]),
                "primitive": str(d["probe"]),
                "n": int(cyc.size),
                "spread_fixed": int(f.max() - f.min()),
                "spread_random": int(r.max() - r.min()),
                "distinct": int(np.unique(cyc).size),
            }
        )
    return rows


def report(rows, table_path=None, experiment="verify"):
    if not rows:
        print(f"[resolution/{experiment}] no captures found")
        return None

    # A primitive is "invariant" when every trace in both classes cost the same.
    invariant = [r for r in rows if r["spread_fixed"] == 0 and r["spread_random"] == 0]
    varying = [r for r in rows if r not in invariant]

    if table_path:
        headers = ["board", "opt", "primitive", "traces", "spread fixed (cyc)",
                   "spread random (cyc)", "distinct values"]
        body = [[r["board"], r["opt"], r["primitive"], str(r["n"]),
                 str(r["spread_fixed"]), str(r["spread_random"]), str(r["distinct"])]
                for r in sorted(rows, key=lambda r: (r["board"], r["opt"], r["primitive"]))]
        write_table(
            headers,
            body,
            table_path,
            table_path.replace(".md", ".tex") if table_path.endswith(".md") else None,
            caption="Observed cycle-count spread per capture. A spread of zero "
                    "cycles across all traces means the implementation's timing "
                    "is invariant over the sampled secret inputs at "
                    "single-cycle resolution, not merely below a detection "
                    "threshold.",
            label=f"tab:resolution{'' if experiment == 'verify' else experiment}",
        )
        print(f"table -> {table_path}")

    print(f"[resolution/{experiment}] {len(rows)} captures: "
          f"{len(invariant)} cycle-invariant (spread = 0), {len(varying)} varying")
    for r in sorted(varying, key=lambda r: -max(r["spread_fixed"], r["spread_random"]))[:8]:
        print(f"   {r['board']}/{r['opt']:<3} {r['primitive']:<26} "
              f"spread fixed={r['spread_fixed']:<6} random={r['spread_random']}")
    for board, hz in CLOCK_HZ.items():
        if any(r["board"] == board for r in rows):
            print(f"   sensitivity on {board}: 1 cycle = {1e9 / hz:.1f} ns "
                  f"at {hz / 1e6:.0f} MHz")
    return rows


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--timing-dir", default="results/timing")
    ap.add_argument("--table", default="results/tables/resolution.md")
    ap.add_argument("--experiment", choices=["verify", "keyed", "both"], default="both")
    a = ap.parse_args()
    for exp in (["verify", "keyed"] if a.experiment == "both" else [a.experiment]):
        tbl = a.table if exp == "verify" else a.table.replace(".md", "_keyed.md")
        report(summarize(a.timing_dir, exp), tbl, exp)


if __name__ == "__main__":
    main()
