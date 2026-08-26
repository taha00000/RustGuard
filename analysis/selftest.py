#!/usr/bin/env python3
"""End-to-end pipeline self-test on SYNTHETIC data â€” no hardware required.

This does NOT produce research results. It fabricates obviously-synthetic inputs
and runs them through the exact same code paths the bench uses â€” every figure and
every table generator â€” so you can confirm the whole chain works before the
hardware arrives. Everything it writes goes to a gitignored directory
(`results/_demo/`) and every figure carries a red SYNTHETIC watermark.

It builds and checks all paper artifacts:
  figures: perf_throughput, perf_overhead, perf_perm, timing, timing_convergence,
           opt_sweep   (6)
  tables : perf_cycles, timing, codesize, opt_sweep   (md + tex)
and asserts the timing t-test flags the leaky control, clears the constant-time
control, and that the opt-level sweep separates a clean level from a leaking one.

  python analysis/selftest.py
"""
from __future__ import annotations

import csv as _csv
import os
import sys

import numpy as np

from parse_perf import parse
from plot_perf import make_all_perf
from dudect import THRESHOLD, make_convergence_figure, make_dudect_figure
from opt_sweep import make_opt_sweep
from tables import _build_from_results
from ct_binary import census_disasm, differential, report as ct_report
from matrix import make_matrix
from figutil import ensure_parent, read_perf_csv

# A synthetic disassembly (rust-objdump format) standing in for the compiled
# ct-probe object: a constant-time decrypt with 2 public-loop branches and a
# variable-time decrypt with 5 (the extra 3 = the secret-dependent early return).
SYNTH_DISASM = """\
00000000 <_ZN14rustguard_core18ascon_aead_decrypt17h0000000000000000E>:
       0: \tcbz\tr0, 0x10 <x>
       4: \tbne\t0x0 <y>
       8: \tbx\tlr

00000020 <_ZN14rustguard_core31ascon_aead_decrypt_variabletime17h1111111111111111E>:
      20: \tbeq\t0x40 <a>
      24: \tbne\t0x40 <b>
      28: \tcbnz\tr2, 0x40 <c>
      2c: \tcbz\tr3, 0x40 <d>
      30: \tblt\t0x40 <e>
      34: \tbx\tlr

00000050 <_ZN14rustguard_core18ascon_aead_encrypt17h2222222222222222E>:
      50: \tbl\t0x100 <s>
      54: \tbx\tlr
"""

WATERMARK = "SYNTHETIC - PIPELINE SELF-TEST - NOT MEASURED DATA"
DEMO = os.path.join("results", "_demo")
SIZES = [8, 16, 32, 64, 128, 256, 512]


def _fake_perf_dump(base: float) -> str:
    lines = ["# RustGuard SYNTHETIC perf dump (self-test, not measured)",
             f"PERM p6 mean_cyc={int(base * 6)}", f"PERM p12 mean_cyc={int(base * 12)}",
             "SECTION:ENCRYPT"]
    for sz in SIZES:
        mean = int(base * sz + base * 24)
        lines.append(f"ENC {sz} mean_cyc={mean} cpb_x100={int(mean * 100 / sz)}")
    lines.append("SECTION:DECRYPT")
    for sz in SIZES:
        lines.append(f"DEC {sz} mean_cyc={int(base * sz + base * 26)}")
    lines.append("SECTION:DONE")
    return "\n".join(lines) + "\n"


def _write_perf_csv(rows, perm, path):
    ensure_parent(path)
    allrows = list(rows) + [{"op": "PERM", "size": int(k[1:]),
                             "mean_cyc": v, "cyc_per_byte": float(v)}
                            for k, v in perm.items()]
    with open(path, "w", newline="") as f:
        w = _csv.DictWriter(f, fieldnames=["op", "size", "mean_cyc", "cyc_per_byte"])
        w.writeheader()
        w.writerows(allrows)


def _synth_timing(rng, n, leak):
    labels = np.tile([0, 1], n // 2).astype(np.uint8)
    cyc = rng.normal(4200.0, 2.0, size=n)
    if leak:
        cyc[labels == 1] = rng.normal(4180.0, 8.0, size=int((labels == 1).sum()))
    return np.rint(cyc).astype(np.uint32), labels


def _save_timing(path, cyc, lab, variant, experiment):
    ensure_parent(path)
    np.savez_compressed(path, cycles=cyc, labels=lab, variant=variant, experiment=experiment)


def main() -> int:
    print(f"=== RustGuard pipeline self-test (SYNTHETIC) -> {DEMO}/ ===")
    rng = np.random.default_rng(20260626)
    figs = os.path.join(DEMO, "figures")
    tbls = os.path.join(DEMO, "tables")
    os.makedirs(figs, exist_ok=True)
    ok = True

    # 1) perf: dump -> parse -> CSV (+PERM) -> 3 figures
    datasets = {}
    for name, base in (("rust", 8.0), ("cref", 6.5), ("pqm4", 4.0)):
        dump = os.path.join(DEMO, "raw", f"perf_{name}.txt")
        ensure_parent(dump)
        with open(dump, "w") as f:
            f.write(_fake_perf_dump(base))
        with open(dump) as f:
            rows, perm = parse(f)
        csv_path = os.path.join(DEMO, f"perf_{name}.csv")
        _write_perf_csv(rows, perm, csv_path)
        datasets[name] = read_perf_csv(csv_path)
    built = make_all_perf(datasets, figs, WATERMARK)
    print(f"  [perf]   {len(built)} figures (throughput, overhead, perm)")
    if len(built) != 3:
        print("  [perf]   FAIL: expected 3 perf figures"); ok = False

    # 2) timing: safe + leaky -> histogram + convergence, assert verdicts
    safe_c, safe_l = _synth_timing(rng, 20000, leak=False)
    leaky_c, leaky_l = _synth_timing(rng, 20000, leak=True)
    safe_npz = os.path.join(DEMO, "timing", "safe.npz")
    leaky_npz = os.path.join(DEMO, "timing", "leaky.npz")
    _save_timing(safe_npz, safe_c, safe_l, "safe-synth", "tagcompare")
    _save_timing(leaky_npz, leaky_c, leaky_l, "leaky-synth", "tagcompare")
    results = make_dudect_figure(safe_npz, leaky_npz, os.path.join(figs, "timing.png"), WATERMARK)
    make_convergence_figure(safe_npz, leaky_npz,
                            os.path.join(figs, "timing_convergence.png"), WATERMARK)
    by = {r["name"].split()[0]: r for r in results}
    if not by["leaky-control"]["leaking"]:
        print("  [timing] FAIL: synthetic leaky control did not trip threshold"); ok = False
    if by["safe"]["leaking"]:
        print("  [timing] FAIL: synthetic constant-time control falsely flagged"); ok = False
    print(f"  [timing] leaky |t|={by['leaky-control']['t']:.1f}, safe |t|={by['safe']['t']:.1f}")

    # 3) opt-level sweep: O0/O1 clean, O2/O3 leak
    levels = {}
    for lvl, leak in (("O0", False), ("O1", False), ("O2", True), ("O3", True)):
        c, l = _synth_timing(rng, 20000, leak=leak)
        p = os.path.join(DEMO, "timing", f"safe_{lvl}.npz")
        _save_timing(p, c, l, f"safe-{lvl}", "tagcompare")
        levels[lvl] = p
    sweep = make_opt_sweep(levels, os.path.join(figs, "opt_sweep.png"),
                           os.path.join(tbls, "opt_sweep.md"), WATERMARK)
    sweep_by = {lvl: (t, leak) for lvl, t, leak in sweep}
    if sweep_by["O0"][1] or not sweep_by["O3"][1]:
        print("  [sweep]  FAIL: opt-sweep did not separate clean O0 from leaking O3"); ok = False
    print(f"  [sweep]  O0 |t|={sweep_by['O0'][0]:.1f} (clean), "
          f"O3 |t|={sweep_by['O3'][0]:.1f} (leaks)")

    # 4) tables: perf_cycles, timing, codesize (from a synthetic size.txt)
    with open(os.path.join(DEMO, "size.txt"), "w") as f:
        f.write("   text\t   data\t    bss\t    dec\t    hex\tfilename\n")
        f.write("  12000\t     16\t   1040\t  13056\t   3300\tfirmware-rust\n")
        f.write("  10800\t     12\t    900\t  11712\t   2dc0\tfirmware-cref\n")
        f.write("   9800\t      8\t    820\t  10628\t   2984\tfirmware-pqm4\n")
    tables_built = _build_from_results(DEMO, tbls)
    names = {os.path.splitext(os.path.basename(t))[0] for t in tables_built}
    print(f"  [tables] {sorted(names)}")
    for need in ("perf_cycles", "timing", "codesize"):
        if need not in names:
            print(f"  [tables] FAIL: missing table {need}"); ok = False

    # 5) binary-level CT census on a synthetic disassembly (no toolchain needed)
    cen = census_disasm(SYNTH_DISASM)
    ct_report(cen, os.path.join(figs, "ct_binary.png"),
              os.path.join(tbls, "ct_binary.md"), WATERMARK)
    diff = differential(cen)
    if not diff or diff[2] <= 0:
        print("  [binary] FAIL: differential did not localize the leaky branch"); ok = False
    else:
        print(f"  [binary] constant-time={diff[0]} cond, variable-time={diff[1]} cond, "
              f"extra={diff[2]} (leak localized)")

    # 6) ecosystem leakage matrix: many primitives x boards x opt levels
    eco_dir = os.path.join(DEMO, "eco_timing")
    prims = ["aes-gcm", "chacha20poly1305", "hmac-sha256", "ascon-aead",
             "LEAKY-control"]
    for board in ("tm4c", "stm32"):
        for opt in ("O0", "O3"):
            for p in prims:
                leak = p == "LEAKY-control"
                c, l = _synth_timing(rng, 2000, leak=leak)
                path = os.path.join(eco_dir, f"{board}_{opt}_{p}.npz")
                ensure_parent(path)
                np.savez_compressed(path, cycles=c, labels=l, variant=p,
                                    experiment="verify", probe=p,
                                    board=board, opt=opt)
    cells = make_matrix(eco_dir, os.path.join(figs, "leakage_matrix.png"),
                        os.path.join(tbls, "leakage_matrix.md"), WATERMARK)
    if not cells:
        print("  [matrix] FAIL: no cells built"); ok = False
    else:
        leaky_cells = [c for c in cells.get("LEAKY-control", {}).values() if c[1]]
        clean_ok = all(not v[1] for p in cells if p != "LEAKY-control"
                       for v in cells[p].values())
        if len(leaky_cells) != 4:
            print("  [matrix] FAIL: leaky control not flagged in all 4 configs"); ok = False
        if not clean_ok:
            print("  [matrix] FAIL: a constant-time primitive was falsely flagged"); ok = False
        print(f"  [matrix] {len(cells)} primitives x 4 configs; "
              f"leaky flagged in {len(leaky_cells)}/4, others clean={clean_ok}")

    print("\n" + ("SELF-TEST PASSED" if ok else "SELF-TEST FAILED"))
    print(f"Built 8 figures + 6 tables under {DEMO}/ (SYNTHETIC, watermarked). "
          "Real results come from the bench via make_figures.py + ct_binary.py.")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

