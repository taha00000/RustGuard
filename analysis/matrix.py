#!/usr/bin/env python3
"""Ecosystem leakage matrix: every primitive x board x optimization level.

The headline artifact of the systematic study. Reads all timing captures
produced by capture/collect_timing.py, computes the dudect t-statistic for each
(primitive, board, optimization level) cell, and emits:

  * a heatmap figure  — green = no timing leakage detected, red = LEAKS
  * a table (Markdown + LaTeX) with the exact |t| values

Cells are read straight from the capture filenames written by the collector
(`<board>_<opt>_<primitive>.npz`) and from the metadata inside each file, so
adding a crate or a board needs no change here.

  python analysis/matrix.py --timing-dir results/timing \
         --fig results/figures/leakage_matrix.png \
         --table results/tables/leakage_matrix.md
"""
from __future__ import annotations

import argparse
import glob
import os

import numpy as np

try:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from matplotlib.colors import ListedColormap
except ImportError:
    plt = None

from dudect import THRESHOLD, load, welch_scalar
from figutil import ensure_parent, watermark, write_table


def collect_cells(timing_dir: str):
    """-> {primitive: {column: (abs_t, leaks)}}, ordered column list."""
    cells: dict[str, dict[str, tuple[float, bool]]] = {}
    columns: list[str] = []
    for path in sorted(glob.glob(os.path.join(timing_dir, "*.npz"))):
        try:
            cyc, lab, variant, _exp = load(path)
        except Exception as e:  # noqa: BLE001 - a bad file shouldn't kill the sweep
            print(f"  skipping {os.path.basename(path)}: {e}")
            continue
        d = np.load(path)
        board = str(d["board"]) if "board" in d else "board"
        opt = str(d["opt"]) if "opt" in d else "O?"
        primitive = str(d["probe"]) if "probe" in d else variant

        t = welch_scalar(cyc[lab == 0], cyc[lab == 1])
        at = float(min(abs(t), 1e3)) if np.isfinite(t) else 1e3
        col = f"{board}/{opt}"
        cells.setdefault(primitive, {})[col] = (at, at > THRESHOLD)
        if col not in columns:
            columns.append(col)
    return cells, sorted(columns)


def _fmt(v: float) -> str:
    if v >= 1e3:
        return "inf"
    return f"{v:.2f}"


def make_matrix(timing_dir, fig_path=None, table_path=None, watermark_text=None):
    cells, columns = collect_cells(timing_dir)
    if not cells:
        print("[matrix] no timing captures found — run capture/collect_timing.py first")
        return None

    # Controls first, then alphabetical, so the validated pair reads at the top.
    def sort_key(name: str):
        return (0 if "LEAKY" in name or "rustguard" in name else 1, name)

    primitives = sorted(cells, key=sort_key)

    # ── table ────────────────────────────────────────────────────────────────
    if table_path:
        headers = ["primitive"] + columns + ["verdict"]
        rows = []
        for p in primitives:
            vals = []
            any_leak = False
            for c in columns:
                if c in cells[p]:
                    v, leak = cells[p][c]
                    vals.append(_fmt(v))
                    any_leak |= leak
                else:
                    vals.append("-")
            rows.append([p] + vals + ["LEAKS" if any_leak else "constant-time"])
        write_table(
            headers,
            rows,
            table_path,
            table_path.replace(".md", ".tex") if table_path.endswith(".md") else None,
            caption=f"Timing-leakage assessment per primitive, board and optimization "
                    f"level (dudect; |t| > {THRESHOLD} indicates leakage).",
            label="tab:leakagematrix",
        )
        print(f"table -> {table_path}")

    # ── heatmap ──────────────────────────────────────────────────────────────
    if fig_path and plt is not None:
        grid = np.full((len(primitives), len(columns)), np.nan)
        for i, p in enumerate(primitives):
            for j, c in enumerate(columns):
                if c in cells[p]:
                    grid[i, j] = 1.0 if cells[p][c][1] else 0.0

        fig_h = max(3.0, 0.45 * len(primitives) + 1.8)
        fig, ax = plt.subplots(figsize=(max(6.0, 1.4 * len(columns) + 3.5), fig_h))
        cmap = ListedColormap(["#2e7d32", "#c62828"])  # green = clean, red = leaks
        ax.imshow(np.nan_to_num(grid, nan=0.5), cmap=cmap, vmin=0, vmax=1, aspect="auto")

        for i, p in enumerate(primitives):
            for j, c in enumerate(columns):
                txt = _fmt(cells[p][c][0]) if c in cells[p] else "-"
                ax.text(j, i, txt, ha="center", va="center", color="white", fontsize=8)

        ax.set_xticks(range(len(columns)))
        ax.set_xticklabels(columns, rotation=30, ha="right")
        ax.set_yticks(range(len(primitives)))
        ax.set_yticklabels(primitives, fontsize=9)
        ax.set_title(f"Timing-leakage matrix (dudect |t|; red = leaks, threshold {THRESHOLD})")
        watermark(fig, watermark_text)
        fig.tight_layout()
        ensure_parent(fig_path)
        fig.savefig(fig_path, dpi=150)
        plt.close(fig)
        print(f"figure -> {fig_path}")

    n_leak = sum(1 for p in cells for c in cells[p] if cells[p][c][1])
    print(f"[matrix] {len(primitives)} primitives x {len(columns)} configs, "
          f"{n_leak} leaking cell(s)")
    return cells


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--timing-dir", default="results/timing")
    ap.add_argument("--fig", default="results/figures/leakage_matrix.png")
    ap.add_argument("--table", default="results/tables/leakage_matrix.md")
    ap.add_argument("--watermark", default=None)
    a = ap.parse_args()
    make_matrix(a.timing_dir, a.fig, a.table, a.watermark)


if __name__ == "__main__":
    main()
