#!/usr/bin/env python3
"""Regenerate every paper figure for which measured inputs exist.

This is the single entry point to run after a bench session. It looks in
`results/` for whatever you have captured and (re)builds the corresponding
figures into `results/figures/`. Missing inputs are skipped with a note, so the
same command works whether you have only the perf data, only the TVLA traces, or
both.

  python analysis/make_figures.py [--results-dir results]

Inputs it looks for
  results/perf_rust.csv  (+ perf_cref.csv, perf_pqm4.csv)  -> figures/perf.png
  results/traces/safe.npz (+ traces/leaky.npz)             -> figures/tvla.png

This script only ever consumes real capture artifacts. To see the pipeline work
before you have hardware, run analysis/selftest.py instead — it is explicitly
synthetic and writes watermarked figures to a separate, gitignored directory.
"""
from __future__ import annotations

import argparse
import os

from figutil import read_perf_csv
from plot_perf import make_perf_figure
from tvla import make_tvla_figure


def build_perf(results_dir: str, fig_dir: str) -> bool:
    candidates = {
        "rust": "perf_rust.csv",
        "cref": "perf_cref.csv",
        "pqm4": "perf_pqm4.csv",
    }
    datasets = {}
    for name, fname in candidates.items():
        path = os.path.join(results_dir, fname)
        if os.path.exists(path):
            datasets[name] = read_perf_csv(path)
    if not datasets:
        print("[perf]  no perf_*.csv found — skipping "
              "(run analysis/parse_perf.py on your UART dumps first)")
        return False
    out = make_perf_figure(datasets, os.path.join(fig_dir, "perf.png"))
    print(f"[perf]  built {out} from: {', '.join(sorted(datasets))}")
    return True


def build_tvla(results_dir: str, fig_dir: str) -> bool:
    safe = os.path.join(results_dir, "traces", "safe.npz")
    leaky = os.path.join(results_dir, "traces", "leaky.npz")
    if not os.path.exists(safe):
        print("[tvla]  no traces/safe.npz found — skipping "
              "(run capture/rustguard_capture/capture_tvla.py first)")
        return False
    leaky = leaky if os.path.exists(leaky) else None
    if leaky is None:
        print("[tvla]  WARNING: no leaky.npz control — the safe result cannot be "
              "validated without it. Capturing the leaky control is mandatory.")
    make_tvla_figure(safe, leaky, os.path.join(fig_dir, "tvla.png"))
    print(f"[tvla]  built {os.path.join(fig_dir, 'tvla.png')}")
    return True


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--results-dir", default="results")
    a = ap.parse_args()
    fig_dir = os.path.join(a.results_dir, "figures")
    os.makedirs(fig_dir, exist_ok=True)

    built = []
    if build_perf(a.results_dir, fig_dir):
        built.append("perf")
    if build_tvla(a.results_dir, fig_dir):
        built.append("tvla")

    if built:
        print(f"\nDone. Built: {', '.join(built)} -> {fig_dir}/")
    else:
        print("\nNo measured inputs found yet. Capture data on the bench, or run "
              "analysis/selftest.py to dry-run the pipeline on synthetic data.")


if __name__ == "__main__":
    main()
