#!/usr/bin/env python3
"""
parse_hw_results.py — parse TM4C123 UART benchmark output and update results.

Usage:
    # Capture UART output to file (e.g., using screen/minicom/putty):
    screen /dev/ttyACM0 115200 | tee results/raw/benchmark_tm4c.txt

    # Then parse and update figures:
    python3 scripts/parse_hw_results.py results/raw/benchmark_tm4c.txt
"""

import sys
import os
import re
from pathlib import Path

REPO_ROOT = Path(__file__).parent.parent

def parse_tm4c_output(filepath: str) -> dict:
    """Parse the TM4C UART output into a structured dict."""
    data = {
        'enc': {},   # size -> (mean_cyc, min_cyc, max_cyc, mean_ns, cpb)
        'dec': {},   # size -> (mean_cyc, min_cyc, max_cyc, mean_ns)
        'pap': {},   # size -> (mean_cyc, min_cyc, max_cyc, mean_ns)
        'p6':  {},   # mean_cyc, mean_ns
        'p12': {},
        'hash': {},  # size -> (mean_cyc, mean_ns)
    }

    section = None
    with open(filepath) as f:
        for line in f:
            line = line.strip()
            if line.startswith('SECTION:'):
                section = line.split(':')[1]
                continue
            if not line or line.startswith('#'):
                continue

            if section == 'PERMUTATION':
                m = re.match(r'PERM\s+(p\d+)\s+mean=(\d+).*?min=(\d+).*?max=(\d+)', line)
                if m:
                    key = m.group(1)
                    data[key] = {
                        'mean_cyc': int(m.group(2)),
                        'min_cyc':  int(m.group(3)),
                        'max_cyc':  int(m.group(4)),
                    }
                m2 = re.match(r'PERM\s+(p\d+)\s+mean_ns=(\d+)', line)
                if m2:
                    data[m2.group(1)]['mean_ns'] = int(m2.group(2))

            elif section == 'ENCRYPT_LATENCY':
                m = re.match(r'ENC_LAT\s+(\d+)\s+mean=(\d+).*?min=(\d+).*?max=(\d+).*?ns=(\d+).*?cpb=(\S+)', line)
                if m:
                    sz = int(m.group(1))
                    data['enc'][sz] = {
                        'mean_cyc': int(m.group(2)),
                        'min_cyc':  int(m.group(3)),
                        'max_cyc':  int(m.group(4)),
                        'mean_ns':  int(m.group(5)),
                        'cpb':      float(m.group(6)),
                    }

            elif section == 'DECRYPT_LATENCY':
                m = re.match(r'DEC_LAT\s+(\d+)\s+mean=(\d+).*?min=(\d+).*?max=(\d+).*?ns=(\d+)', line)
                if m:
                    sz = int(m.group(1))
                    data['dec'][sz] = {
                        'mean_cyc': int(m.group(2)),
                        'min_cyc':  int(m.group(3)),
                        'max_cyc':  int(m.group(4)),
                        'mean_ns':  int(m.group(5)),
                    }

            elif section == 'PAP_LATENCY':
                m = re.match(r'PAP_LAT\s+(\d+)\s+mean=(\d+).*?min=(\d+).*?max=(\d+).*?ns=(\d+)', line)
                if m:
                    sz = int(m.group(1))
                    data['pap'][sz] = {
                        'mean_cyc': int(m.group(2)),
                        'min_cyc':  int(m.group(3)),
                        'max_cyc':  int(m.group(4)),
                        'mean_ns':  int(m.group(5)),
                    }

            elif section == 'HASH_LATENCY':
                m = re.match(r'HASH_LAT\s+(\d+)\s+mean=(\d+).*?ns=(\d+)', line)
                if m:
                    sz = int(m.group(1))
                    data['hash'][sz] = {
                        'mean_cyc': int(m.group(2)),
                        'mean_ns':  int(m.group(3)),
                    }

    return data


def print_summary(data: dict):
    print("\n=== TM4C123 Benchmark Summary (Cortex-M4F @ 80 MHz) ===\n")

    if data['p6']:
        print(f"Permutation p6:  {data['p6']['mean_cyc']} cycles  ({data['p6'].get('mean_ns','?')} ns)")
    if data['p12']:
        print(f"Permutation p12: {data['p12']['mean_cyc']} cycles  ({data['p12'].get('mean_ns','?')} ns)")
    print()

    sizes = sorted(data['enc'].keys())
    print(f"{'Size':>5}  {'Enc cyc':>8}  {'Enc ns':>8}  {'cyc/B':>7}  {'Dec ns':>8}  {'PAP ns':>8}")
    print(f"{'-----':>5}  {'--------':>8}  {'------':>8}  {'-----':>7}  {'------':>8}  {'------':>8}")
    for sz in sizes:
        e = data['enc'].get(sz, {})
        d = data['dec'].get(sz, {})
        p = data['pap'].get(sz, {})
        print(f"{sz:>5}  "
              f"{e.get('mean_cyc','?'):>8}  "
              f"{e.get('mean_ns','?'):>8}  "
              f"{e.get('cpb','?'):>7}  "
              f"{d.get('mean_ns','?'):>8}  "
              f"{p.get('mean_ns','?'):>8}")


def write_results_file(data: dict, outpath: str):
    """Write a structured results file compatible with the paper."""
    lines = [
        "# RustGuard TM4C123GH6PM Hardware Benchmark Results",
        "# MCU: TM4C123GH6PM (ARM Cortex-M4F @ 80 MHz)",
        "# Flash: 256 KB  |  SRAM: 32 KB",
        "# Rust 1.75.0, opt-level=3, lto=true, codegen-units=1",
        f"# Iterations: 500  Warmup: 50",
        "# 1 cycle = 12.5 ns at 80 MHz",
        "",
    ]

    if data['p6'] or data['p12']:
        lines.append("SECTION:PERMUTATION")
        if data['p6']:
            lines.append(f"PERM p6  mean_cyc={data['p6']['mean_cyc']}  mean_ns={data['p6'].get('mean_ns','?')}")
        if data['p12']:
            lines.append(f"PERM p12 mean_cyc={data['p12']['mean_cyc']}  mean_ns={data['p12'].get('mean_ns','?')}")
        lines.append("")

    if data['enc']:
        lines.append("SECTION:ENCRYPT_LATENCY")
        lines.append("# size_bytes  mean_cyc  min_cyc  max_cyc  mean_ns  cyc_per_byte")
        for sz in sorted(data['enc']):
            e = data['enc'][sz]
            lines.append(f"ENC_LAT {sz:3}  {e['mean_cyc']}  {e['min_cyc']}  {e['max_cyc']}  {e.get('mean_ns','?')}  {e.get('cpb','?')}")
        lines.append("")

    if data['dec']:
        lines.append("SECTION:DECRYPT_LATENCY")
        for sz in sorted(data['dec']):
            d = data['dec'][sz]
            lines.append(f"DEC_LAT {sz:3}  {d['mean_cyc']}  {d['min_cyc']}  {d['max_cyc']}  {d.get('mean_ns','?')}")
        lines.append("")

    if data['pap']:
        lines.append("SECTION:PAP_LATENCY")
        for sz in sorted(data['pap']):
            p = data['pap'][sz]
            lines.append(f"PAP_LAT {sz:3}  {p['mean_cyc']}  {p['min_cyc']}  {p['max_cyc']}  {p.get('mean_ns','?')}")
        lines.append("")

    Path(outpath).parent.mkdir(parents=True, exist_ok=True)
    with open(outpath, 'w') as f:
        f.write('\n'.join(lines))
    print(f"\nFormatted results written to: {outpath}")


if __name__ == '__main__':
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)

    filepath = sys.argv[1]
    print(f"Parsing: {filepath}")
    data = parse_tm4c_output(filepath)
    print_summary(data)
    outpath = str(REPO_ROOT / 'results' / 'raw' / 'benchmark_tm4c_parsed.txt')
    write_results_file(data, outpath)
    print("\nNext: run python3 scripts/generate_figures.py to update all plots")
