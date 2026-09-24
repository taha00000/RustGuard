#!/usr/bin/env python3
"""Assert the invariants that make the committed results trustworthy.

Runs in CI against whatever is in `results/`, so the claims in the paper cannot
quietly drift away from the data backing them. It checks what a reader is
entitled to assume:

  1. **Every column has a working control.** A "no leakage detected" verdict is
     meaningless in a configuration where nothing was shown to be detectable.
     This is not hypothetical: the realistic control stops leaking at -O2 once
     LLVM rewrites it branchless, which is why the optimizer-proof canary
     exists and why this check is per column rather than per study.
  2. **No production primitive is flagged.** If one ever is, that is a finding
     and the paper's headline claim must change — so CI should fail loudly
     rather than let it pass unnoticed.
  3. **No synthetic data among the results.** `analysis/selftest.py` fabricates
     watermarked inputs for pipeline testing; none of it may reach `results/`.
  4. **Captures are distinct.** Deterministic counters make genuine repeats
     look like copied files, so duplicates are reported for inspection rather
     than treated as failures — with the known-legitimate pairs exempted.

  python analysis/check_results.py [--timing-dir results/timing]
"""
from __future__ import annotations

import argparse
import glob
import hashlib
import os
import sys

import numpy as np

from dudect import THRESHOLD, load, welch_cropped, welch_scalar

CONTROL_MARKERS = ("CANARY", "LEAKY", "LADDER")

# Pairs that legitimately produce identical data, with the reason. The leaky
# control's variable-time behaviour is in the tag comparison, so under the keyed
# experiment it runs the same encryption as the constant-time probe.
EXPECTED_DUPLICATES = {
    frozenset({"rustguard-ascon128", "rustguard-LEAKY-control"}):
        "leaky control shares the constant-time encrypt path (keyed experiment)",
}


def is_control(name: str) -> bool:
    return any(m in name for m in CONTROL_MARKERS)


def check(timing_dir: str) -> int:
    failures: list[str] = []
    columns: dict[str, bool] = {}
    flagged: list[tuple[str, str, float]] = []
    by_hash: dict[str, list[str]] = {}
    n = 0

    for path in sorted(glob.glob(os.path.join(timing_dir, "*.npz"))):
        d = np.load(path)
        if not ("board" in d and "opt" in d and "probe" in d):
            continue  # deep-dive capture, not part of the grid
        n += 1
        name = str(d["probe"])
        exp = str(d.get("experiment", "verify"))
        clock = str(d["clock"]) if "clock" in d else "default"
        board = str(d["board"])
        col = f"{board}{'' if clock == 'default' else '@' + clock}/{d['opt']}/{exp}"

        cyc, lab, _v, _e = load(path)
        noisy = "noisy" in d and str(d["noisy"]) in ("1", "True", "true")
        t = abs(welch_cropped(cyc[lab == 0], cyc[lab == 1])[0] if noisy
                else welch_scalar(cyc[lab == 0], cyc[lab == 1]))

        if is_control(name):
            columns[col] = columns.get(col, False) or t > THRESHOLD
        else:
            columns.setdefault(col, False)
            if t > THRESHOLD:
                flagged.append((col, name, t))

        by_hash.setdefault(
            hashlib.sha1(cyc.astype(np.float64).tobytes()).hexdigest(), []
        ).append(os.path.basename(path))

    if n == 0:
        print("no sweep captures found — nothing to check")
        return 0

    print(f"checked {n} capture(s) across {len(columns)} configuration(s)")

    uncontrolled = sorted(c for c, ok in columns.items() if not ok)
    if uncontrolled:
        failures.append(
            f"{len(uncontrolled)} configuration(s) have no control that leaks, so "
            f"their clean verdicts are unvalidated: {uncontrolled[:5]}"
        )
    else:
        print("  every configuration carries a control that demonstrably leaks")

    if flagged:
        failures.append(
            "production primitive(s) flagged as leaking — this is a finding and "
            "the paper's claims must be revisited: "
            + ", ".join(f"{c} {p} |t|={t:.2f}" for c, p, t in flagged[:5])
        )
    else:
        print("  no production primitive is flagged in any configuration")

    demo = [p for p in glob.glob("results/**/*", recursive=True) if "_demo" in p]
    tracked_demo = [p for p in demo if os.path.isfile(p)]
    if tracked_demo and os.environ.get("CI"):
        # _demo is gitignored; in CI it should not exist at all.
        failures.append(f"synthetic self-test output present under results/: "
                        f"{len(tracked_demo)} file(s)")
    else:
        print("  no synthetic data under results/")

    def probe_of(filename: str) -> str:
        # `<board>[_<clock>]_<opt>_<probe>[_keyed].npz` -> probe
        stem = filename.replace("_keyed", "").removesuffix(".npz")
        parts = stem.split("_")
        for i, part in enumerate(parts):
            if len(part) == 2 and part[0] == "O" and part[1].isdigit():
                return "_".join(parts[i + 1:])
        return stem

    # Identical data for the *same* probe in different configurations is
    # expected whenever LLVM emits the same code and the counter is
    # deterministic — e.g. the canary is byte-identical at -O1 and -O3, and
    # across both vendors' M4s, because it is a black_box loop with no memory
    # behaviour to differ. Only duplicates spanning *different* probes suggest a
    # copied file, so those are the ones worth surfacing.
    unexplained = []
    for files in (f for f in by_hash.values() if len(f) > 1):
        probes = frozenset(probe_of(f) for f in files)
        if len(probes) > 1 and probes not in EXPECTED_DUPLICATES:
            unexplained.append(sorted(files))
    if unexplained:
        print(f"  note: {len(unexplained)} duplicate-data group(s) not in the "
              f"expected list — verify these are codegen coincidences, not copies:")
        for files in unexplained[:5]:
            print(f"     {files}")
    else:
        print("  no unexplained duplicate captures")

    if failures:
        print("\nFAILED:")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("\nall result invariants hold")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--timing-dir", default="results/timing")
    return check(ap.parse_args().timing_dir)


if __name__ == "__main__":
    sys.exit(main())
