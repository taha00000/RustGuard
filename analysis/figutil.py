#!/usr/bin/env python3
"""Small shared helpers for the RustGuard figure scripts.

Kept dependency-light (numpy + matplotlib only) so the analysis pipeline runs on
any host without the ChipWhisperer stack installed.
"""
from __future__ import annotations

import csv
import os


def watermark(fig, text: str) -> None:
    """Stamp a diagonal watermark across a figure.

    Used by the self-test to make absolutely sure synthetic demo figures can
    never be mistaken for measured results. Real figures pass text=None and get
    no stamp.
    """
    if not text:
        return
    fig.text(
        0.5,
        0.5,
        text,
        fontsize=34,
        color="red",
        alpha=0.18,
        ha="center",
        va="center",
        rotation=30,
        weight="bold",
        zorder=1000,
    )


def read_perf_csv(path: str):
    """Read a perf CSV (op,size,mean_cyc,cyc_per_byte) into a list of dict rows."""
    rows = []
    with open(path, newline="") as f:
        for r in csv.DictReader(f):
            rows.append(
                {
                    "op": r["op"],
                    "size": int(r["size"]),
                    "mean_cyc": int(r["mean_cyc"]),
                    "cyc_per_byte": float(r["cyc_per_byte"]),
                }
            )
    return rows


def ensure_parent(path: str) -> None:
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
