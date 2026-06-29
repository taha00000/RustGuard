#!/usr/bin/env python3
"""Binary-level constant-time analysis of the compiled thumbv7em crypto.

The *static* leg of the source -> binary -> silicon triangulation. It disassembles
the actual compiled Cortex-M object (the ct-probe staticlib) and censuses, per
function, the control-flow constructs that can carry data-dependent timing:
conditional branches (b<cond>, cbz/cbnz), IT (if-then) blocks, and variable-
latency instructions (udiv/sdiv). ASCON's permutation is pure fixed-count loops
and bitwise ops, so a constant-time build should show only public-loop branches.

The signal is the safe-vs-leaky *differential*: the constant-time decrypt
(subtle::ct_eq) and the variable-time decrypt (early-return byte compare) differ
only in the tag check, so any extra conditional branches in the variable-time
function localize exactly the secret-dependent control flow. That differential is
the binary-level analogue of the dudect timing result and the opt-level sweep.

Honest scope: this is a control-flow census + differential, not a sound taint-
tracking proof (cf. BINSEC/Rel). It is validated by its controls — the leaky
variant must show extra branches, the constant-time one must not — and is meant
to be triangulated with the on-silicon dudect measurement, not to replace a
formal proof.

  # from a built staticlib (runs rust-objdump / llvm-objdump for you)
  python analysis/ct_binary.py --elf <target>/thumbv7em-none-eabihf/release/libct_probe.a

  # or from a saved disassembly
  rust-objdump -d --no-show-raw-insn lib.a > d.txt
  python analysis/ct_binary.py --disasm d.txt
"""
from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys

try:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
except ImportError:
    plt = None

try:
    from figutil import ensure_parent, watermark, write_table
except ImportError:
    def watermark(_f, _t):
        pass

# Conditional (data-dependent-capable) control flow on thumb.
_COND = {"beq", "bne", "bcs", "bcc", "bhs", "blo", "bmi", "bpl", "bvs", "bvc",
         "bhi", "bls", "bge", "blt", "bgt", "ble", "cbz", "cbnz"}
_DIV = {"udiv", "sdiv"}
_FUNC_RE = re.compile(r"^[0-9a-fA-F]+\s+<(.+)>:")
_INSN_RE = re.compile(r"^\s*[0-9a-fA-F]+:\s+(\S+)")

# The crypto functions we report on, keyed by a suffix of the demangled name.
CRYPTO = {
    "ascon_aead_decrypt_variabletime": "decrypt (variable-time, leaky)",
    "ascon_aead_decrypt": "decrypt (constant-time)",
    "ascon_aead_encrypt": "encrypt",
    "decrypt_core": "decrypt core (permutation)",
    "ascon_p": "permutation p",
}


def demangle(sym: str) -> str:
    """Minimal legacy-Rust (_ZN..E) demangler; drops the trailing hash segment."""
    if not sym.startswith("_ZN"):
        return sym
    i, parts = 3, []
    while i < len(sym) and sym[i].isdigit():
        j = i
        while j < len(sym) and sym[j].isdigit():
            j += 1
        n = int(sym[i:j])
        parts.append(sym[j:j + n])
        i = j + n
    parts = [p for p in parts if not re.fullmatch(r"h[0-9a-f]{16}", p)]
    return "::".join(parts) if parts else sym


def census_disasm(text: str) -> dict:
    """Per-function control-flow census. Pure and unit-tested."""
    out = {}
    cur = None
    for ln in text.splitlines():
        fm = _FUNC_RE.match(ln)
        if fm:
            cur = demangle(fm.group(1))
            out[cur] = {"cond": 0, "it": 0, "div": 0}
            continue
        if cur is None:
            continue
        im = _INSN_RE.match(ln)
        if not im:
            continue
        mn = im.group(1).lower()
        if mn in _COND:
            out[cur]["cond"] += 1
        elif mn in _DIV:
            out[cur]["div"] += 1
        elif re.fullmatch(r"it[te]{0,3}", mn):
            out[cur]["it"] += 1
    return out


def _match_crypto(census: dict):
    """Return {label: (full_name, counts)} for the crypto functions present."""
    found = {}
    for full, counts in census.items():
        for suffix, label in CRYPTO.items():
            if full.endswith(suffix):
                # keep the most specific (longest suffix) match per function
                found.setdefault(full, (label, counts))
                break
    return found


def differential(census: dict):
    """Extra conditional branches in the variable-time decrypt vs the constant-
    time one — the localized secret-dependent control flow. Returns (ct, var, delta)
    or None if both are not present."""
    ct = var = None
    for full, c in census.items():
        if full.endswith("ascon_aead_decrypt_variabletime"):
            var = c["cond"]
        elif full.endswith("ascon_aead_decrypt"):
            ct = c["cond"]
    if ct is None or var is None:
        return None
    return ct, var, var - ct


def report(census: dict, fig_path=None, table_path=None, watermark_text=None):
    crypto = _match_crypto(census)
    rows = sorted(([lbl, c["cond"], c["it"], c["div"]]
                   for _f, (lbl, c) in crypto.items()), key=lambda r: r[0])
    for lbl, cond, it, div in rows:
        print(f"  {lbl:<32} cond-branches={cond:<3} IT={it:<3} div={div}")
    diff = differential(census)
    if diff:
        ct, var, delta = diff
        verdict = ("DETECTED" if delta > 0 else "not separated")
        print(f"\n  safe-vs-leaky differential: constant-time={ct} cond, "
              f"variable-time={var} cond, extra={delta} ({verdict})")

    if table_path:
        write_table(["function", "cond branches", "IT blocks", "udiv/sdiv"], rows,
                    table_path,
                    table_path.replace(".md", ".tex") if table_path.endswith(".md") else None,
                    caption="Control-flow census of the compiled thumbv7em crypto "
                            "(constant-time vs variable-time decrypt).",
                    label="tab:ctbinary")
        print(f"table -> {table_path}")

    if fig_path and plt is not None and rows:
        fig, ax = plt.subplots(figsize=(7, 4.2))
        labels = [r[0] for r in rows]
        conds = [r[1] for r in rows]
        colors = ["C3" if "leaky" in l else "C0" for l in labels]
        ax.barh(labels, conds, color=colors)
        ax.set_xlabel("conditional branches in the compiled binary")
        ax.set_title("Binary-level control-flow census (thumbv7em)")
        ax.grid(True, axis="x", ls=":", alpha=0.5)
        watermark(fig, watermark_text)
        fig.tight_layout()
        ensure_parent(fig_path)
        fig.savefig(fig_path, dpi=150)
        plt.close(fig)
        print(f"figure -> {fig_path}")
    return crypto, diff


def find_objdump(explicit=None):
    for cand in ([explicit] if explicit else []) + ["rust-objdump", "llvm-objdump"]:
        if cand and shutil.which(cand):
            return cand
    # fall back to the toolchain sysroot (CI after `rustup component add llvm-tools`)
    try:
        root = subprocess.check_output(["rustc", "--print", "sysroot"],
                                       text=True).strip()
        for base, _d, files in os.walk(os.path.join(root, "lib", "rustlib")):
            for f in files:
                if f in ("llvm-objdump", "llvm-objdump.exe"):
                    return os.path.join(base, f)
    except Exception:
        pass
    return None


def disassemble(elf_path, objdump=None) -> str:
    tool = find_objdump(objdump)
    if not tool:
        sys.exit("no objdump found. Install with `rustup component add llvm-tools` "
                 "or pass --objdump <path>.")
    return subprocess.check_output([tool, "-d", "--no-show-raw-insn", elf_path],
                                   text=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--elf", help="compiled object/staticlib to disassemble")
    g.add_argument("--disasm", help="a saved `objdump -d` text file")
    ap.add_argument("--objdump", help="objdump binary (default: auto-detect)")
    ap.add_argument("--fig", default="results/figures/ct_binary.png")
    ap.add_argument("--table", default="results/tables/ct_binary.md")
    ap.add_argument("--watermark", default=None)
    a = ap.parse_args()
    text = disassemble(a.elf, a.objdump) if a.elf else open(a.disasm).read()
    report(census_disasm(text), a.fig, a.table, a.watermark)


if __name__ == "__main__":
    main()
