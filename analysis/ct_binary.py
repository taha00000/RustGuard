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
tracking proof (cf. BINSEC/Rel). It is validated by its controls â€” the leaky
variant must show extra branches, the constant-time one must not â€” and is meant
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


_ESCAPES = {
    "$LT$": "<", "$GT$": ">", "$u20$": " ", "$RF$": "&", "$C$": ",",
    "$u7b$": "{", "$u7d$": "}", "$LP$": "(", "$RP$": ")", "$BP$": "*",
    "$u5b$": "[", "$u5d$": "]", "$u27$": "'",
}


def _unescape(s: str) -> str:
    for k, v in _ESCAPES.items():
        s = s.replace(k, v)
    return s.replace("..", "::")


def _demangle_v0(sym: str):
    """Extract a readable path from a Rust v0 (`_R...`) symbol.

    v0 encodes the crate root as `Cs<disambiguator>_<len><name>` followed by
    length-prefixed path segments. Full v0 demangling is involved; we only need
    the crate and function path for attribution.
    """
    m = re.search(r"Cs[0-9A-Za-z]+_(\d+)", sym)
    if not m:
        return None
    n = int(m.group(1))
    i = m.end()
    parts = [sym[i:i + n]]
    i += n
    while i < len(sym):
        j = i
        while j < len(sym) and sym[j].isdigit():
            j += 1
        if j == i:
            break
        seg_len = int(sym[i:j])
        seg = sym[j:j + seg_len]
        if not seg:
            break
        parts.append(seg)
        i = j + seg_len
    return "::".join(parts) if parts else None


def demangle(sym: str) -> str:
    """Best-effort Rust demangler covering legacy (`_ZN..E`), v0 (`_R..`), and the
    `$LT$`/`$u20$` escapes LLVM emits for trait-impl symbols."""
    if sym.startswith("_ZN"):
        i, parts = 3, []
        while i < len(sym) and sym[i].isdigit():
            j = i
            while j < len(sym) and sym[j].isdigit():
                j += 1
            n = int(sym[i:j])
            parts.append(sym[j:j + n])
            i = j + n
        parts = [p for p in parts if not re.fullmatch(r"h[0-9a-f]{16}", p)]
        return _unescape("::".join(parts)) if parts else sym
    if sym.startswith("_R"):
        v0 = _demangle_v0(sym)
        if v0:
            return _unescape(v0)
    return _unescape(sym)


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
    """Return {full_name: (label, counts)} for the known control-pair functions."""
    found = {}
    for full, counts in census.items():
        for suffix, label in CRYPTO.items():
            if full.endswith(suffix):
                # keep the most specific (longest suffix) match per function
                found.setdefault(full, (label, counts))
                break
    return found


# Symbols that are runtime/harness infrastructure, not crypto under test. They
# would otherwise dominate the census (the UART command loop and core::fmt are
# branch-heavy) and bury the result.
_IGNORE_CRATES = {
    "core", "compiler_builtins", "cortex_m", "cortex_m_rt", "panic_halt",
    "alloc", "rustc_std_workspace_core", "firmware_tm4c", "firmware_stm32_timing",
    "<other>",
}
_IGNORE_PREFIXES = ("__aeabi", "__ARM", "$", "Reset", "HardFault", "DefaultHandler")

# The firmware is built with `lto = true`, so most of a crypto crate's code is
# inlined into the `probes::p_<x>::{verify,correct}` wrapper that calls it.
# Attributing those wrappers back to the primitive under test is essential —
# otherwise the census reports near-zero for every crate and hides the signal.
_PROBE_CRATE = {
    "p_rustguard": "rustguard-ascon128",
    "p_rustguard_leaky": "rustguard-LEAKY-control",
    "p_ascon_rc": "ascon-aead",
    "p_chachapoly": "chacha20poly1305",
    "p_aesgcm": "aes-gcm",
    "p_aesgcmsiv": "aes-gcm-siv",
    "p_aeseax": "eax-aes128",
    "p_aesccm": "ccm-aes128",
    "p_hmac_sha256": "hmac-sha256",
    "p_cmac_aes": "cmac-aes128",
}


def crate_of(name: str) -> str:
    """Owning crate/primitive of a demangled symbol.

    Handles plain paths (`aes_gcm::foo`), trait-impl forms
    (`<chacha20poly1305::X as aead::AeadInPlace>::method` -> the crate being
    implemented *for*, not the trait's crate), and probe wrappers, which are
    attributed to the primitive they exercise (see `_PROBE_CRATE`).
    """
    parts = name.split("::")
    if len(parts) >= 2 and parts[0] == "probes" and parts[1] in _PROBE_CRATE:
        return _PROBE_CRATE[parts[1]]
    # LLVM emits trait-impl symbols as `_<T as Trait>::m`; strip the leading
    # underscore/angle/reference so the implementing type is what we read.
    n = name.lstrip("_").lstrip("<").lstrip("&")
    n = n.split(" as ", 1)[0]
    n = re.split(r"[<>(,\[]", n, maxsplit=1)[0]
    return n.split("::", 1)[0].strip() if "::" in n else "<other>"


def _is_infra(name: str) -> bool:
    return (crate_of(name) in _IGNORE_CRATES
            or name.startswith(_IGNORE_PREFIXES))


def by_crate(census: dict) -> dict:
    """Aggregate the census per crate: totals plus the worst single function.

    This is the ecosystem-scale view: for each crate, how much
    potentially-secret-dependent control flow does the *compiled* code contain?
    """
    agg: dict = {}
    for name, c in census.items():
        crate = crate_of(name)
        if _is_infra(name):
            continue
        a = agg.setdefault(crate, {"cond": 0, "it": 0, "div": 0,
                                   "fns": 0, "worst": ("", 0)})
        a["cond"] += c["cond"]
        a["it"] += c["it"]
        a["div"] += c["div"]
        a["fns"] += 1
        if c["cond"] > a["worst"][1]:
            a["worst"] = (name, c["cond"])
    return agg


def triage(census: dict, top: int = 15):
    """Rank functions by constructs that can carry data-dependent timing.

    IMPORTANT â€” this is a *screening* heuristic, not a leakage detector. Every
    real implementation contains public, loop-bounded branches; a high count does
    not prove a secret-dependent branch exists. Its job is to decide which
    primitives to put on the bench first. Ground truth is the on-hardware dudect
    measurement (capture/collect_timing.py + analysis/matrix.py). A sound static
    answer would need taint tracking (cf. BINSEC/Rel), which is out of scope.

    `div` is weighted heavily: on Cortex-M4 `UDIV`/`SDIV` take 2-12 cycles
    depending on operand values, so division on secret data leaks directly.
    """
    scored = []
    for name, c in census.items():
        if _is_infra(name):
            continue
        score = c["cond"] + 2 * c["it"] + 10 * c["div"]
        if score:
            scored.append((score, name, c))
    scored.sort(key=lambda r: -r[0])
    return scored[:top]


def differential(census: dict):
    """Extra conditional branches in the variable-time decrypt vs the constant-
    time one â€” the localized secret-dependent control flow. Returns (ct, var, delta)
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


def report_ecosystem(census: dict, table_path=None, top: int = 15):
    """Crate-level census + triage ranking across every primitive in the image."""
    agg = by_crate(census)
    if not agg:
        print("  (no crate symbols found)")
        return agg

    print(f"  {'crate':<26} {'fns':>5} {'cond':>6} {'IT':>5} {'div':>5}   worst function")
    rows = []
    for crate, a in sorted(agg.items(), key=lambda kv: -kv[1]["cond"]):
        worst = a["worst"][0].split("::")[-1][:34] if a["worst"][0] else "-"
        print(f"  {crate:<26} {a['fns']:>5} {a['cond']:>6} {a['it']:>5} "
              f"{a['div']:>5}   {worst}")
        rows.append([crate, a["fns"], a["cond"], a["it"], a["div"], worst])

    hot = triage(census, top)
    if hot:
        print(f"\n  triage â€” functions to measure first (screening heuristic, "
              f"div weighted x10):")
        for score, name, c in hot[:top]:
            short = name if len(name) <= 60 else "..." + name[-57:]
            print(f"    score={score:<5} cond={c['cond']:<4} it={c['it']:<3} "
                  f"div={c['div']:<3} {short}")

    if table_path:
        write_table(
            ["crate", "functions", "cond branches", "IT blocks", "udiv/sdiv",
             "worst function"],
            rows, table_path,
            table_path.replace(".md", ".tex") if table_path.endswith(".md") else None,
            caption="Per-crate control-flow census of the compiled thumbv7em image. "
                    "A screening heuristic for prioritising hardware measurement, "
                    "not a leakage detector.",
            label="tab:ctecosystem")
        print(f"\ntable -> {table_path}")
    return agg


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
    ap.add_argument("--ecosystem", action="store_true",
                    help="per-crate census + triage ranking across the whole image")
    ap.add_argument("--eco-table", default="results/tables/ct_ecosystem.md")
    ap.add_argument("--top", type=int, default=15, help="triage entries to show")
    ap.add_argument("--watermark", default=None)
    a = ap.parse_args()
    text = disassemble(a.elf, a.objdump) if a.elf else open(a.disasm).read()
    census = census_disasm(text)
    report(census, a.fig, a.table, a.watermark)
    if a.ecosystem:
        print()
        report_ecosystem(census, a.eco_table, a.top)


if __name__ == "__main__":
    main()

