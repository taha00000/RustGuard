#!/usr/bin/env python3
"""Parse TM4C UART perf output into a tidy CSV for plotting.

Reads the raw serial dump captured from firmware-tm4c (via an external USB-UART
dongle at 115200 8N1) and emits results/perf_tm4c.csv.
"""
import argparse, csv, re, sys

def parse(lines):
    rows = []
    section = None
    perm = {}
    for ln in lines:
        ln = ln.strip()
        if ln.startswith("PERM"):
            m = re.match(r"PERM (\w+) mean_cyc=(\d+)", ln)
            if m: perm[m.group(1)] = int(m.group(2))
        elif ln.startswith("SECTION:"):
            section = ln.split(":",1)[1]
        elif ln.startswith("ENC") or ln.startswith("DEC"):
            m = re.match(r"(ENC|DEC) (\d+) mean_cyc=(\d+)", ln)
            if m:
                op, sz, cyc = m.group(1), int(m.group(2)), int(m.group(3))
                rows.append({"op": op, "size": sz, "mean_cyc": cyc,
                             "cyc_per_byte": round(cyc/sz, 2)})
    return rows, perm

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("infile", help="raw UART dump (text)")
    ap.add_argument("--out", default="results/perf_tm4c.csv")
    a = ap.parse_args()
    with open(a.infile) as f:
        rows, perm = parse(f)
    if not rows:
        sys.exit("no ENC/DEC lines found — check the UART dump")
    import os; os.makedirs(os.path.dirname(a.out) or ".", exist_ok=True)
    with open(a.out, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["op","size","mean_cyc","cyc_per_byte"])
        w.writeheader(); w.writerows(rows)
    print(f"p6={perm.get('p6')} p12={perm.get('p12')} cyc")
    print(f"wrote {len(rows)} rows -> {a.out}")

if __name__ == "__main__":
    main()
