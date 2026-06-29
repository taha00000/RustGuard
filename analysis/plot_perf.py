#!/usr/bin/env python3
"""Performance figures: throughput, cost of memory safety, and permutation cost.

Consumes the per-implementation CSVs produced by analysis/parse_perf.py (one per
firmware build, all measured on the same TM4C123 board and toolchain) and emits
three figures:

  perf_throughput.png  encrypt cycles/byte vs payload size, Rust vs C vs asm
  perf_overhead.png    Rust-over-baseline overhead (the cost of memory safety)
  perf_perm.png        p6/p12 permutation cycles per implementation

  python analysis/plot_perf.py \
      --csv rust=results/perf_rust.csv \
      --csv cref=results/perf_cref.csv \
      --csv pqm4=results/perf_pqm4.csv \
      --outdir results/figures

At least one --csv is required. The overhead figure needs `rust` plus at least
one baseline (`cref` and/or `pqm4`).
"""
from __future__ import annotations

import argparse
import os
import sys

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402

from figutil import ensure_parent, read_perf_csv, watermark  # noqa: E402

# Stable colour/marker per implementation so figures are comparable run to run.
STYLE = {
    "rust": ("C0", "o", "Rust (forbid(unsafe))"),
    "cref": ("C1", "s", "ASCON reference C"),
    "pqm4": ("C2", "^", "pqm4 assembly"),
}


def _series(rows, op):
    s = sorted((r for r in rows if r["op"] == op), key=lambda r: r["size"])
    return [r["size"] for r in s], [r["cyc_per_byte"] for r in s], [r["mean_cyc"] for r in s]


def make_throughput_figure(datasets, out, watermark_text=None):
    fig, ax = plt.subplots(figsize=(6.5, 4.2))
    for name, rows in datasets.items():
        color, marker, label = STYLE.get(name, ("C7", "x", name))
        sizes, cpb, _ = _series(rows, "ENC")
        if sizes:
            ax.plot(sizes, cpb, marker=marker, color=color, label=label)
    ax.set_xscale("log", base=2)
    ax.set_xlabel("payload (bytes)")
    ax.set_ylabel("cycles / byte (encrypt)")
    ax.set_title("ASCON-128 encrypt throughput, same TM4C123 board")
    ax.grid(True, which="both", ls=":", alpha=0.5)
    ax.legend()
    watermark(fig, watermark_text)
    fig.tight_layout()
    ensure_parent(out)
    fig.savefig(out, dpi=150)
    plt.close(fig)
    return out


def make_overhead_figure(datasets, out, watermark_text=None):
    if "rust" not in datasets:
        return None
    rust = {r["size"]: r["cyc_per_byte"] for r in datasets["rust"] if r["op"] == "ENC"}
    fig, ax = plt.subplots(figsize=(6.5, 4.2))
    plotted = False
    for base in ("cref", "pqm4"):
        if base not in datasets:
            continue
        b = {r["size"]: r["cyc_per_byte"] for r in datasets[base] if r["op"] == "ENC"}
        sizes = sorted(set(rust) & set(b))
        ratio = [rust[s] / b[s] for s in sizes if b[s]]
        color, marker, label = STYLE.get(base, ("C7", "x", base))
        ax.plot(sizes, ratio, marker=marker, color=color, label=f"Rust / {label}")
        plotted = True
    if not plotted:
        plt.close(fig)
        return None
    ax.axhline(1.0, color="k", ls="--", lw=0.8, label="parity")
    ax.set_xscale("log", base=2)
    ax.set_xlabel("payload (bytes)")
    ax.set_ylabel("overhead factor (x)")
    ax.set_title("Cost of memory safety (Rust / baseline)")
    ax.grid(True, which="both", ls=":", alpha=0.5)
    ax.legend()
    watermark(fig, watermark_text)
    fig.tight_layout()
    ensure_parent(out)
    fig.savefig(out, dpi=150)
    plt.close(fig)
    return out


def make_perm_figure(datasets, out, watermark_text=None):
    rounds = [6, 12]
    names = [n for n in datasets if any(r["op"] == "PERM" for r in datasets[n])]
    if not names:
        return None
    fig, ax = plt.subplots(figsize=(6.5, 4.2))
    width = 0.8 / len(names)
    x = np.arange(len(rounds))
    for i, name in enumerate(names):
        perm = {r["size"]: r["mean_cyc"] for r in datasets[name] if r["op"] == "PERM"}
        vals = [perm.get(r, 0) for r in rounds]
        color, _, label = STYLE.get(name, ("C7", "x", name))
        ax.bar(x + i * width, vals, width, color=color, label=label)
    ax.set_xticks(x + width * (len(names) - 1) / 2)
    ax.set_xticklabels([f"p{r}" for r in rounds])
    ax.set_ylabel("cycles per permutation")
    ax.set_title("ASCON permutation cost (p6 / p12)")
    ax.grid(True, axis="y", ls=":", alpha=0.5)
    ax.legend()
    watermark(fig, watermark_text)
    fig.tight_layout()
    ensure_parent(out)
    fig.savefig(out, dpi=150)
    plt.close(fig)
    return out


def make_all_perf(datasets, outdir, watermark_text=None):
    """Build every perf figure that the data supports. Returns paths written."""
    built = []
    for fn, fname in (
        (make_throughput_figure, "perf_throughput.png"),
        (make_overhead_figure, "perf_overhead.png"),
        (make_perm_figure, "perf_perm.png"),
    ):
        p = fn(datasets, os.path.join(outdir, fname), watermark_text)
        if p:
            built.append(p)
    return built


def _parse_csv_args(pairs):
    datasets = {}
    for p in pairs:
        if "=" not in p:
            sys.exit(f"--csv expects name=path, got {p!r}")
        name, path = p.split("=", 1)
        datasets[name] = read_perf_csv(path)
    return datasets


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--csv", action="append", default=[], metavar="name=path",
                    help="implementation CSV (repeatable; rust/cref/pqm4 are styled)")
    ap.add_argument("--outdir", default="results/figures")
    ap.add_argument("--watermark", default=None)
    a = ap.parse_args()
    if not a.csv:
        ap.error("at least one --csv name=path is required")
    built = make_all_perf(_parse_csv_args(a.csv), a.outdir, a.watermark)
    for p in built:
        print(f"perf figure -> {p}")


if __name__ == "__main__":
    main()
