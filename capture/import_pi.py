#!/usr/bin/env python3
"""Turn a pi-runner CSV into the same .npz captures the boards produce.

The Raspberry Pi harness writes plain CSV (it runs on the Pi, where numpy may
not be installed), so this converts it on the analysis machine into exactly the
format `analysis/matrix.py`, `analysis/dudect.py` and `analysis/resolution.py`
already consume. One pipeline then covers microcontrollers and application
cores alike.

Two fields matter for honest reporting and are carried through from the CSV
header: `counter` (which clock the samples came from — PMU core cycles, the
coarse architectural counter, or nanoseconds) and `noisy`, set for application
cores so the analysis knows the samples carry OS and microarchitectural noise
rather than being deterministic.

  python capture/import_pi.py pi5_keyed.csv --opt O3 --outdir results/timing
"""
from __future__ import annotations

import argparse
import csv
import os
from collections import defaultdict

import numpy as np


def read_csv(path):
    """-> (meta dict, {probe: (cycles list, labels list)})"""
    meta = {}
    data = defaultdict(lambda: ([], []))
    with open(path, newline="") as f:
        rows = []
        for line in f:
            if line.startswith("#"):
                k, _, v = line[1:].strip().partition("=")
                meta[k.strip()] = v.strip()
            else:
                rows.append(line)
    reader = csv.DictReader(rows)
    for r in reader:
        cyc, lab = data[r["probe"]]
        cyc.append(int(r["cycles"]))
        lab.append(int(r["label"]))
    return meta, data


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("csv", help="file written by pi-runner --out")
    ap.add_argument("--board", help="override the board label from the CSV header")
    ap.add_argument("--opt", default="O3", help="optimization level the runner was built at")
    ap.add_argument("--clock", default="default", help="clock label, if varied")
    ap.add_argument("--outdir", default="results/timing")
    a = ap.parse_args()

    meta, data = read_csv(a.csv)
    if not data:
        raise SystemExit(f"no samples found in {a.csv}")
    board = a.board or meta.get("board", "pi")
    experiment = meta.get("experiment", "verify")
    counter = meta.get("counter", "unknown")

    os.makedirs(a.outdir, exist_ok=True)
    written = 0
    for probe, (cyc, lab) in sorted(data.items()):
        cycles = np.asarray(cyc, dtype=np.uint64)
        labels = np.asarray(lab, dtype=np.uint8)
        suffix = "_keyed" if experiment == "keyed" else ""
        clk = "" if a.clock == "default" else f"_{a.clock}"
        path = os.path.join(a.outdir, f"{board}{clk}_{a.opt}_{probe}{suffix}.npz")
        np.savez_compressed(
            path,
            cycles=cycles,
            labels=labels,
            variant=probe,
            experiment=experiment,
            probe=probe,
            board=board,
            opt=a.opt,
            clock=a.clock,
            counter=counter,
            # Application cores are not deterministic: a zero spread is not
            # expected here, and the analysis should crop outliers rather than
            # treat every sample as exact.
            noisy="1",
            model=meta.get("model", ""),
            governor=meta.get("governor", ""),
        )
        written += 1
        f, r = cycles[labels == 0], cycles[labels == 1]
        print(f"  {probe:<26} n={cycles.size:<8} fixed={f.mean():>12.1f} "
              f"random={r.mean():>12.1f} -> {os.path.basename(path)}")

    print(f"imported {written} capture(s) from {a.csv} "
          f"[board={board} experiment={experiment} counter={counter}]")


if __name__ == "__main__":
    main()
