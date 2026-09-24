#!/usr/bin/env python3
"""How well does binary-level screening predict measured timing behaviour?

Static constant-time screening — counting conditional branches, IT blocks and
variable-latency instructions in a disassembly — is cheap and needs no hardware,
so it is a tempting way to triage a dependency tree. This module asks how much
that screening is actually worth by joining it against the on-silicon ground
truth from the same binary:

  * `analysis/ct_binary.py --ecosystem` gives, per crate, the constructs that
    *could* carry data-dependent timing;
  * the leakage matrix gives what the hardware actually did.

A crate carrying many conditional branches that measures perfectly
cycle-invariant is a **false positive** of the static screen: the branches are
there, but they are driven by public values (block counts, buffer lengths), not
by secrets. Quantifying that rate is the point — it is the difference between
"this heuristic prioritises measurement" and "this heuristic detects leaks".

  python analysis/static_vs_measured.py --elf <firmware.elf> \
         --timing-dir results/timing --table results/tables/static_vs_measured.md
"""
from __future__ import annotations

import argparse
import os

from ct_binary import by_crate, census_disasm, disassemble
from figutil import write_table
from matrix import collect_cells

# Census crate names -> probe names in the measurement matrix.
ALIASES = {
    "aes_gcm": "aes-gcm",
    "aes_gcm_siv": "aes-gcm-siv",
    "chacha20poly1305": "chacha20poly1305",
    "ascon_aead": "ascon-aead",
    "eax": "eax-aes128",
    "ccm": "ccm-aes128",
    "hmac": "hmac-sha256",
    "sha2": "hmac-sha256",
    "cmac": "cmac-aes128",
    "rustguard_core": "rustguard-ascon128",
    "aes": "aes-gcm",
    "polyval": "aes-gcm-siv",
    "ghash": "aes-gcm",
    "poly1305": "chacha20poly1305",
}


def join(census_agg, cells):
    """-> rows of (primitive, cond, it, div, measured verdict, agreement)."""
    static = {}
    for crate, c in census_agg.items():
        name = ALIASES.get(crate, crate.replace("_", "-"))
        s = static.setdefault(name, {"cond": 0, "it": 0, "div": 0, "crates": set()})
        s["cond"] += c.get("cond", 0)
        s["it"] += c.get("it", 0)
        s["div"] += c.get("div", 0)
        s["crates"].add(crate)

    rows = []
    for name in sorted(set(static) | set(cells)):
        s = static.get(name, {"cond": 0, "it": 0, "div": 0})
        measured = cells.get(name)
        if measured is None:
            continue  # a crate with no probe of its own (e.g. a shared backend)
        leaks = any(v[1] for v in measured.values())
        flagged = s["cond"] > 0 or s["div"] > 0
        if flagged and not leaks:
            agreement = "false positive"
        elif flagged and leaks:
            agreement = "true positive"
        elif not flagged and leaks:
            agreement = "MISSED"
        else:
            agreement = "true negative"
        rows.append([name, str(s["cond"]), str(s["it"]), str(s["div"]),
                     "leaks" if leaks else "cycle-invariant", agreement])
    return rows


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--elf", required=True, help="a built firmware ELF to disassemble")
    ap.add_argument("--timing-dir", default="results/timing")
    ap.add_argument("--table", default="results/tables/static_vs_measured.md")
    a = ap.parse_args()

    if not os.path.exists(a.elf):
        raise SystemExit(f"no such ELF: {a.elf}")
    agg = by_crate(census_disasm(disassemble(a.elf)))
    cells, _cols = collect_cells(a.timing_dir, "verify")
    rows = join(agg, cells)
    if not rows:
        raise SystemExit("nothing to join — are there captures in the timing dir?")

    write_table(
        ["primitive", "cond branches", "IT blocks", "variable-latency div",
         "measured", "screening outcome"],
        rows,
        a.table,
        a.table.replace(".md", ".tex") if a.table.endswith(".md") else None,
        caption="Binary-level constant-time screening against measured ground "
                "truth. Crates carrying conditional branches that measure "
                "cycle-invariant are false positives of the static screen: the "
                "branches are driven by public values, not secrets.",
        label="tab:staticvsmeasured",
    )
    print(f"table -> {a.table}")
    fp = sum(1 for r in rows if r[5] == "false positive")
    missed = sum(1 for r in rows if r[5] == "MISSED")
    print(f"[static-vs-measured] {len(rows)} primitives: {fp} false positive(s), "
          f"{missed} missed leak(s)")
    for r in rows:
        print(f"   {r[0]:<26} cond={r[1]:<5} div={r[3]:<3} {r[4]:<16} {r[5]}")


if __name__ == "__main__":
    main()
