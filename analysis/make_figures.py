#!/usr/bin/env python3
"""Regenerate every paper figure and table for which measured inputs exist.

The single entry point to run after a bench session. It looks in `results/` for
whatever you captured and (re)builds the corresponding figures into
`results/figures/` and tables into `results/tables/`. Missing inputs are skipped
with a note, so the same command works whether you have only perf data, only
timing data, or the full set.

  python analysis/make_figures.py [--results-dir results]

Inputs it looks for
  results/perf_{rust,cref,pqm4}.csv     -> figures/perf_{throughput,overhead,perm}.png
                                           tables/perf_cycles.{md,tex}
  results/timing/{safe,leaky}.npz       -> figures/timing.png, timing_convergence.png
                                           tables/timing.{md,tex}
  results/timing/safe_O{0..3}.npz       -> figures/opt_sweep.png, tables/opt_sweep.{md,tex}
  results/size.txt (arm-none-eabi-size) -> tables/codesize.{md,tex}

Only consumes real capture artifacts. To see the pipeline work before hardware,
run analysis/selftest.py — it is explicitly synthetic and writes watermarked
output to a separate, gitignored directory.
"""
from __future__ import annotations

import argparse
import glob
import os
import re

from figutil import read_perf_csv
from plot_perf import make_all_perf
from dudect import make_convergence_figure, make_dudect_figure
from opt_sweep import make_opt_sweep
from tables import _build_from_results
from ct_binary import census_disasm, disassemble, report as ct_report


def build_perf(results_dir, fig_dir):
    datasets = {}
    for name in ("rust", "cref", "pqm4"):
        p = os.path.join(results_dir, f"perf_{name}.csv")
        if os.path.exists(p):
            datasets[name] = read_perf_csv(p)
    if not datasets:
        print("[perf]   no perf_*.csv — skipping (run analysis/parse_perf.py first)")
        return False
    built = make_all_perf(datasets, fig_dir)
    print(f"[perf]   {len(built)} figure(s) from: {', '.join(sorted(datasets))}")
    return True


def build_timing(results_dir, fig_dir):
    safe = os.path.join(results_dir, "timing", "safe.npz")
    leaky = os.path.join(results_dir, "timing", "leaky.npz")
    if not os.path.exists(safe):
        print("[timing] no timing/safe.npz — skipping (run capture/collect_timing.py)")
        return False
    leaky = leaky if os.path.exists(leaky) else None
    if leaky is None:
        print("[timing] WARNING: no leaky.npz control — safe result cannot be validated")
    make_dudect_figure(safe, leaky, os.path.join(fig_dir, "timing.png"))
    make_convergence_figure(safe, leaky, os.path.join(fig_dir, "timing_convergence.png"))
    return True


def build_ct_binary(results_dir, fig_dir, tbl_dir):
    # Prefer a saved disassembly; else disassemble the built ct-probe staticlib.
    disasm = os.path.join(results_dir, "ct_disasm.txt")
    text = None
    if os.path.exists(disasm):
        text = open(disasm).read()
    else:
        for libname in ("libct_probe.a", "ct_probe.lib"):
            for base, _d, files in os.walk("."):
                if "thumbv7em" in base and libname in files:
                    try:
                        text = disassemble(os.path.join(base, libname))
                    except Exception:
                        text = None
                    break
            if text:
                break
    if not text:
        print("[binary] no ct_disasm.txt or built ct-probe staticlib — skipping "
              "(see docs: build ct-probe, rust-objdump -d > results/ct_disasm.txt)")
        return False
    ct_report(census_disasm(text), os.path.join(fig_dir, "ct_binary.png"),
              os.path.join(tbl_dir, "ct_binary.md"))
    return True


def build_opt_sweep(results_dir, fig_dir, tbl_dir):
    found = {}
    for p in sorted(glob.glob(os.path.join(results_dir, "timing", "safe_O*.npz"))):
        m = re.search(r"safe_(O\d)\.npz$", os.path.basename(p))
        if m:
            found[m.group(1)] = p
    if len(found) < 2:
        print("[sweep]  <2 opt-level captures (timing/safe_O*.npz) — skipping")
        return False
    make_opt_sweep(found, os.path.join(fig_dir, "opt_sweep.png"),
                   os.path.join(tbl_dir, "opt_sweep.md"))
    return True


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--results-dir", default="results")
    a = ap.parse_args()
    fig_dir = os.path.join(a.results_dir, "figures")
    tbl_dir = os.path.join(a.results_dir, "tables")
    os.makedirs(fig_dir, exist_ok=True)
    os.makedirs(tbl_dir, exist_ok=True)

    built = []
    if build_perf(a.results_dir, fig_dir):
        built.append("perf")
    if build_timing(a.results_dir, fig_dir):
        built.append("timing")
    if build_ct_binary(a.results_dir, fig_dir, tbl_dir):
        built.append("ct-binary")
    if build_opt_sweep(a.results_dir, fig_dir, tbl_dir):
        built.append("opt-sweep")

    tables = _build_from_results(a.results_dir, tbl_dir)
    if tables:
        built.append(f"{len({os.path.splitext(os.path.basename(t))[0] for t in tables})} table(s)")

    if built:
        print(f"\nDone. Built: {', '.join(built)} -> {fig_dir}/ and {tbl_dir}/")
    else:
        print("\nNo measured inputs found yet. Capture data on the bench, or run "
              "analysis/selftest.py to dry-run the pipeline on synthetic data.")


if __name__ == "__main__":
    main()
