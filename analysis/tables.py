#!/usr/bin/env python3
"""Generate the paper's result tables (Markdown for the README, LaTeX to \\input).

Three tables, all from real bench artifacts:

  perf_cycles   per-payload-size cycle counts + cyc/byte, Rust vs C vs asm, and
                the Rust-over-fastest-baseline overhead.
  timing        per timing capture: peak |t|, verdict, fixed/random mean cycles.
  codesize      flash/RAM footprint per implementation, from arm-none-eabi-size.

These are library functions; make_figures.py calls them with whatever inputs
exist. The CLI builds them from a results/ directory.
"""
from __future__ import annotations

import argparse
import os

import numpy as np

from figutil import parse_size_output, read_perf_csv, write_table
from dudect import load, welch_scalar, THRESHOLD

IMPL_ORDER = ["rust", "cref", "pqm4"]
IMPL_LABEL = {"rust": "Rust", "cref": "C ref", "pqm4": "pqm4 asm"}


def perf_cycle_table(datasets: dict, md_path, tex_path=None):
    """datasets: {impl: rows}. One row per ENC payload size."""
    impls = [i for i in IMPL_ORDER if i in datasets] + \
            [i for i in datasets if i not in IMPL_ORDER]
    enc = {i: {r["size"]: r for r in datasets[i] if r["op"] == "ENC"} for i in impls}
    sizes = sorted({s for i in impls for s in enc[i]})
    headers = ["bytes"] + [f"{IMPL_LABEL.get(i, i)} cyc" for i in impls] + \
              ["Rust cyc/B", "overhead vs best"]
    rows = []
    for s in sizes:
        cyc = [enc[i].get(s, {}).get("mean_cyc", "") for i in impls]
        rust_cpb = enc.get("rust", {}).get(s, {}).get("cyc_per_byte", "")
        bases = [enc[i][s]["cyc_per_byte"] for i in impls
                 if i != "rust" and s in enc[i]]
        if rust_cpb != "" and bases:
            overhead = f"{rust_cpb / min(bases):.2f}x"
        else:
            overhead = "-"
        rows.append([s] + cyc + [rust_cpb, overhead])
    return write_table(headers, rows, md_path, tex_path,
                       caption="ASCON-128 encrypt cycle counts on the TM4C123 "
                               "(same board and toolchain).",
                       label="tab:perf")


def timing_table(npz_paths, md_path, tex_path=None):
    headers = ["capture", "experiment", "peak |t|", "verdict",
               "fixed mean", "random mean"]
    rows = []
    for p in npz_paths:
        cyc, lab, variant, experiment = load(p)
        t = welch_scalar(cyc[lab == 0], cyc[lab == 1])
        td = "inf" if np.isinf(t) else f"{abs(t):.2f}"
        rows.append([variant, experiment, td,
                     "LEAKS" if abs(t) > THRESHOLD else "constant-time",
                     f"{cyc[lab == 0].mean():.1f}", f"{cyc[lab == 1].mean():.1f}"])
    return write_table(headers, rows, md_path, tex_path,
                       caption="Timing-leakage assessment (dudect, "
                               f"|t|>{THRESHOLD} = leak).",
                       label="tab:timing")


def codesize_table(size_dicts, md_path, tex_path=None):
    """size_dicts: list of {name, flash, ram} (from parse_size_output)."""
    headers = ["binary", "flash (B)", "RAM (B)"]
    rows = [[d["name"], d["flash"], d["ram"]] for d in size_dicts]
    return write_table(headers, rows, md_path, tex_path,
                       caption="Code size (flash) and static RAM per build, "
                               "from arm-none-eabi-size.",
                       label="tab:codesize")


def _build_from_results(results_dir, out_dir):
    built = []
    # perf table
    datasets = {}
    for name in ("rust", "cref", "pqm4"):
        p = os.path.join(results_dir, f"perf_{name}.csv")
        if os.path.exists(p):
            datasets[name] = read_perf_csv(p)
    if datasets:
        built += perf_cycle_table(datasets, os.path.join(out_dir, "perf_cycles.md"),
                                  os.path.join(out_dir, "perf_cycles.tex"))
    # timing table
    tdir = os.path.join(results_dir, "timing")
    npzs = [os.path.join(tdir, f) for f in ("safe.npz", "leaky.npz")
            if os.path.exists(os.path.join(tdir, f))]
    if npzs:
        built += timing_table(npzs, os.path.join(out_dir, "timing.md"),
                              os.path.join(out_dir, "timing.tex"))
    # code-size table
    sz = os.path.join(results_dir, "size.txt")
    if os.path.exists(sz):
        with open(sz) as f:
            dicts = parse_size_output(f.read())
        if dicts:
            built += codesize_table(dicts, os.path.join(out_dir, "codesize.md"),
                                    os.path.join(out_dir, "codesize.tex"))
    return built


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--results-dir", default="results")
    ap.add_argument("--out-dir", default="results/tables")
    a = ap.parse_args()
    built = _build_from_results(a.results_dir, a.out_dir)
    if built:
        for p in built:
            print(f"table -> {p}")
    else:
        print("no table inputs found in", a.results_dir)


if __name__ == "__main__":
    main()
