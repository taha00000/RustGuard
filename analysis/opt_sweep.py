#!/usr/bin/env python3
"""Optimization-level sweep: peak |t| vs compiler optimization level.

The central experiment. Source-level constant-time code can be preserved or
broken by the optimizer, so the constant-time DUT is rebuilt at several
optimization levels, a timing capture is taken for each, and the peak |t| is
plotted against the level. A bar that stays under 4.5 across all levels is the
"it held" result; a bar that crosses 4.5 at some level localizes where the
compiler reintroduced data-dependent timing.

Inputs are one timing .npz per level, named on the command line:

  python analysis/opt_sweep.py \
      --level O0=results/timing/safe_O0.npz \
      --level O1=results/timing/safe_O1.npz \
      --level O2=results/timing/safe_O2.npz \
      --level O3=results/timing/safe_O3.npz \
      --plot results/figures/opt_sweep.png \
      --table results/tables/opt_sweep.md
"""
from __future__ import annotations

import argparse
import sys

import numpy as np

try:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
except ImportError:
    plt = None

from dudect import THRESHOLD, load, welch_scalar
from figutil import ensure_parent, watermark, write_table


def peak_t(npz_path) -> float:
    cyc, lab, _v, _e = load(npz_path)
    t = welch_scalar(cyc[lab == 0], cyc[lab == 1])
    return float(min(abs(t), 1e3)) if np.isfinite(t) else 1e3


def make_opt_sweep(levels: dict, plot=None, table=None, watermark_text=None):
    """levels: ordered {label: npz_path}. Returns [(label, peak_t, leaks), ...]."""
    rows = [(lvl, peak_t(path), peak_t(path) > THRESHOLD) for lvl, path in levels.items()]

    if plot and plt is not None:
        fig, ax = plt.subplots(figsize=(6.5, 4.2))
        labels = [r[0] for r in rows]
        vals = [r[1] for r in rows]
        colors = ["C3" if r[2] else "C0" for r in rows]
        ax.bar(labels, vals, color=colors)
        ax.axhline(THRESHOLD, color="r", ls="--", lw=0.9, label=f"threshold {THRESHOLD}")
        ax.set_ylabel("peak |t|")
        ax.set_xlabel("optimization level")
        ax.set_title("Does constant-time survive the optimizer?  peak |t| per -O level")
        ax.grid(True, axis="y", ls=":", alpha=0.5)
        ax.legend()
        watermark(fig, watermark_text)
        fig.tight_layout()
        ensure_parent(plot)
        fig.savefig(plot, dpi=150)
        plt.close(fig)
        print(f"figure -> {plot}")

    if table:
        write_table(
            ["opt level", "peak |t|", "verdict"],
            [[lvl, f"{t:.2f}", "LEAKS" if leak else "constant-time"] for lvl, t, leak in rows],
            table,
            table.replace(".md", ".tex") if table.endswith(".md") else None,
            caption="Peak timing-leakage t-statistic per optimization level.",
            label="tab:optsweep",
        )
        print(f"table -> {table}")
    return rows


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--level", action="append", default=[], metavar="LABEL=path",
                    help="one timing npz per optimization level (repeatable)")
    ap.add_argument("--plot", default="results/figures/opt_sweep.png")
    ap.add_argument("--table", default="results/tables/opt_sweep.md")
    ap.add_argument("--watermark", default=None)
    a = ap.parse_args()
    if not a.level:
        ap.error("at least one --level LABEL=path is required")
    levels = {}
    for spec in a.level:
        if "=" not in spec:
            sys.exit(f"--level expects LABEL=path, got {spec!r}")
        k, v = spec.split("=", 1)
        levels[k] = v
    make_opt_sweep(levels, a.plot, a.table, a.watermark)


if __name__ == "__main__":
    main()
