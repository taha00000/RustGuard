#!/usr/bin/env python3
"""Small shared helpers for the RustGuard figure scripts.

Kept dependency-light (numpy + matplotlib only) so the analysis pipeline runs on
any host without the serial-collection dependencies installed.
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


def write_table(headers, rows, md_path, tex_path=None, caption="", label=""):
    """Write a table as GitHub Markdown and (optionally) a LaTeX booktabs table.

    `rows` is a list of lists of cells (str/number). Returns the paths written so
    the paper can \\input the .tex directly and the README can show the .md.
    """
    ensure_parent(md_path)
    with open(md_path, "w", encoding="utf-8") as f:
        f.write("| " + " | ".join(str(h) for h in headers) + " |\n")
        f.write("|" + "|".join("---" for _ in headers) + "|\n")
        for r in rows:
            f.write("| " + " | ".join(str(c) for c in r) + " |\n")
    written = [md_path]

    if tex_path:
        ensure_parent(tex_path)
        with open(tex_path, "w", encoding="utf-8") as f:
            f.write("\\begin{table}[t]\n  \\centering\n")
            if caption:
                f.write(f"  \\caption{{{caption}}}\n")
            if label:
                f.write(f"  \\label{{{label}}}\n")
            f.write("  \\begin{tabular}{" + "l" * len(headers) + "}\n    \\toprule\n")
            f.write("    " + " & ".join(str(h) for h in headers) + " \\\\\n    \\midrule\n")
            for r in rows:
                f.write("    " + " & ".join(str(c) for c in r) + " \\\\\n")
            f.write("    \\bottomrule\n  \\end{tabular}\n\\end{table}\n")
        written.append(tex_path)
    return written


def parse_size_output(text: str):
    """Parse `arm-none-eabi-size` (Berkeley format) output into per-binary dicts.

      text   data    bss     dec     hex   filename
      12345    16   1024   13385    3449   firmware-tm4c
    -> [{"name": "firmware-tm4c", "text": 12345, "data": 16, "bss": 1024,
         "flash": 12361, "ram": 1040}]
    flash = text + data (lives in flash); ram = data + bss (lives in SRAM).
    """
    out = []
    for ln in text.splitlines():
        parts = ln.split()
        if len(parts) < 6 or parts[0] == "text" or not parts[0].isdigit():
            continue
        t, d, b = int(parts[0]), int(parts[1]), int(parts[2])
        out.append({"name": parts[5], "text": t, "data": d, "bss": b,
                    "flash": t + d, "ram": d + b})
    return out
