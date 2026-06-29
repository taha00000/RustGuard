#!/usr/bin/env python3
"""Performance figure: cost of memory safety, Rust vs C vs asm.

Consumes the per-implementation CSVs produced by analysis/parse_perf.py (one per
firmware build, all measured on the same TM4C123 board and toolchain) and emits
the encrypt cycles/byte-vs-size comparison plus the Rust-over-baseline overhead.

  python analysis/plot_perf.py \
      --csv rust=results/perf_rust.csv \
      --csv cref=results/perf_cref.csv \
      --csv pqm4=results/perf_pqm4.csv \
      --out results/figures/perf.png

At least one --csv is required. The overhead annotation is computed only when a
baseline named `cref` and/or `pqm4` is present alongside `rust`.
"""
from __future__ import annotations

import argparse
import sys

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

from figutil import ensure_parent, read_perf_csv, watermark  # noqa: E402

# Stable colour/marker per implementation so figures are comparable run to run.
STYLE = {
    "rust": ("C0", "o", "Rust (forbid(unsafe))"),
    "cref": ("C1", "s", "ASCON reference C"),
    "pqm4": ("C2", "^", "pqm4 assembly"),
}


def _enc_series(rows):
    enc = sorted((r for r in rows if r["op"] == "ENC"), key=lambda r: r["size"])
    return [r["size"] for r in enc], [r["cyc_per_byte"] for r in enc]


def make_perf_figure(datasets: dict, out: str, watermark_text: str | None = None):
    """datasets: {impl_name: rows}. Writes the figure to `out`."""
    have_overhead = "rust" in datasets and (
        "cref" in datasets or "pqm4" in datasets
    )
    fig, axes = plt.subplots(
        1, 2 if have_overhead else 1, figsize=(11 if have_overhead else 6, 4.2),
        squeeze=False,
    )
    ax = axes[0][0]

    for name, rows in datasets.items():
        color, marker, label = STYLE.get(name, ("C7", "x", name))
        sizes, cpb = _enc_series(rows)
        if sizes:
            ax.plot(sizes, cpb, marker=marker, color=color, label=label)
    ax.set_xscale("log", base=2)
    ax.set_xlabel("payload (bytes)")
    ax.set_ylabel("cycles / byte (encrypt)")
    ax.set_title("ASCON-128 encrypt throughput, same TM4C123 board")
    ax.grid(True, which="both", ls=":", alpha=0.5)
    ax.legend()

    if have_overhead:
        ax2 = axes[0][1]
        rust = {r["size"]: r["cyc_per_byte"] for r in datasets["rust"] if r["op"] == "ENC"}
        for base in ("cref", "pqm4"):
            if base not in datasets:
                continue
            b = {r["size"]: r["cyc_per_byte"] for r in datasets[base] if r["op"] == "ENC"}
            sizes = sorted(set(rust) & set(b))
            ratio = [rust[s] / b[s] for s in sizes if b[s]]
            color, marker, label = STYLE.get(base, ("C7", "x", base))
            ax2.plot(sizes, ratio, marker=marker, color=color,
                     label=f"Rust / {label}")
        ax2.axhline(1.0, color="k", ls="--", lw=0.8)
        ax2.set_xscale("log", base=2)
        ax2.set_xlabel("payload (bytes)")
        ax2.set_ylabel("overhead factor (x)")
        ax2.set_title("Cost of memory safety (Rust / baseline)")
        ax2.grid(True, which="both", ls=":", alpha=0.5)
        ax2.legend()

    watermark(fig, watermark_text)
    fig.tight_layout()
    ensure_parent(out)
    fig.savefig(out, dpi=150)
    plt.close(fig)
    return out


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
                    help="implementation CSV, e.g. rust=results/perf_rust.csv "
                         "(repeatable; names rust/cref/pqm4 are styled)")
    ap.add_argument("--out", default="results/figures/perf.png")
    ap.add_argument("--watermark", default=None,
                    help="diagonal stamp (used by the self-test for synthetic data)")
    a = ap.parse_args()
    if not a.csv:
        ap.error("at least one --csv name=path is required")
    datasets = _parse_csv_args(a.csv)
    out = make_perf_figure(datasets, a.out, a.watermark)
    print(f"perf figure -> {out}")


if __name__ == "__main__":
    main()
