#!/usr/bin/env python3
"""Detection floor: how small a timing leak can each platform actually see?

"No leakage detected" is only meaningful next to a sensitivity. The ladder
probes inject leaks of known, geometrically increasing size (1, 2, 4, ... 256
extra loop iterations when a secret bit is set), so the smallest rung a platform
still flags is that platform's detection floor for this method.

The floor is a property of the *platform and the measurement*, not of the crates:

  * on an in-order, cacheless microcontroller with interrupts masked and an
    exact cycle counter, the counts are deterministic and the floor is expected
    to be a single cycle;
  * on an OS-scheduled application core with caches, branch prediction and
    frequency scaling, preemption and microarchitectural noise raise it by
    orders of magnitude, even with percentile cropping.

Reporting this converts every clean verdict in the matrix from "we saw nothing"
into "any leak above N cycles on this platform would have been seen".

  python analysis/ladder.py --timing-dir results/timing \
         --table results/tables/detection_floor.md
"""
from __future__ import annotations

import argparse
import glob
import os
import re

import numpy as np

from dudect import THRESHOLD, load, welch_cropped, welch_scalar
from figutil import write_table

LADDER_RE = re.compile(r"^(?P<board>.+?)_(?P<opt>O\d)_LADDER-(?P<mag>\d+)(?P<keyed>_keyed)?\.npz$")


def collect(timing_dir: str):
    """-> {column: {nominal_iters: (delta_cycles, abs_t, detected)}}"""
    out: dict[str, dict[int, tuple[float, float, bool]]] = {}
    for path in sorted(glob.glob(os.path.join(timing_dir, "*LADDER-*.npz"))):
        m = LADDER_RE.match(os.path.basename(path))
        if not m:
            continue
        d = np.load(path)
        cyc, lab, _v, _e = load(path)
        f, r = cyc[lab == 0], cyc[lab == 1]
        noisy = "noisy" in d and str(d["noisy"]) in ("1", "True", "true")
        t = welch_cropped(f, r)[0] if noisy else welch_scalar(f, r)
        at = abs(t)
        clock = str(d["clock"]) if "clock" in d else "default"
        board = str(d["board"]) if "board" in d else m.group("board")
        col = f"{board}/{m.group('opt')}" if clock == "default" else f"{board}@{clock}/{m.group('opt')}"
        delta = abs(float(f.mean()) - float(r.mean()))
        out.setdefault(col, {})[int(m.group("mag"))] = (delta, at, at > THRESHOLD)
    return out


def report(cols, table_path=None):
    if not cols:
        print("[ladder] no LADDER-* captures found — build with --features ladder")
        return None

    headers = ["platform", "nominal iterations", "measured delta (counter units)",
               "|t|", "detected"]
    rows = []
    floors = {}
    for col in sorted(cols):
        rungs = cols[col]
        detected = [m for m in sorted(rungs) if rungs[m][2]]
        floors[col] = min(detected) if detected else None
        for mag in sorted(rungs):
            delta, at, det = rungs[mag]
            rows.append([col, str(mag), f"{delta:.2f}",
                         "inf" if not np.isfinite(at) else f"{at:.2f}",
                         "yes" if det else "no"])

    if table_path:
        write_table(
            headers, rows, table_path,
            table_path.replace(".md", ".tex") if table_path.endswith(".md") else None,
            caption="Calibrated leak ladder. Each rung injects a leak of known "
                    "size; the smallest rung still flagged is the platform's "
                    "detection floor for this method, and bounds what a clean "
                    "verdict elsewhere in the study can claim.",
            label="tab:detectionfloor",
        )
        print(f"table -> {table_path}")

    print("[ladder] detection floor per platform:")
    for col, floor in sorted(floors.items()):
        if floor is None:
            print(f"   {col:<22} no rung detected — the method is blind here")
        else:
            delta = cols[col][floor][0]
            print(f"   {col:<22} smallest detected leak: {floor} iteration(s) "
                  f"= {delta:.2f} counter units")
    return floors


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--timing-dir", default="results/timing")
    ap.add_argument("--table", default="results/tables/detection_floor.md")
    a = ap.parse_args()
    report(collect(a.timing_dir), a.table)


if __name__ == "__main__":
    main()
