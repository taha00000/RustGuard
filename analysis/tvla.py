#!/usr/bin/env python3
"""Welch's t-test TVLA analysis for RustGuard power traces.

Implements the standard fixed-vs-random first-order TVLA test (Goodwill et al.,
"A testing methodology for side-channel resistance validation", 2011):

  t(j) = (mean_fixed[j] - mean_random[j]) /
         sqrt(var_fixed[j]/n_fixed + var_random[j]/n_random)

A device is flagged as leaking at sample j if |t(j)| > 4.5 (the conventional
threshold giving a very low false-positive rate over thousands of samples).

This script consumes the .npz produced by capture/capture_tvla.py. Run it twice
— once on the leaky-control capture and once on the safe (constant-time) capture
— and compare:

  * leaky control  : MUST exceed +/-4.5 (validates the whole measurement chain)
  * safe DUT       : the actual research result

If the leaky control does NOT trip the threshold, the setup is broken and the
safe result means nothing. The paper must report both.
"""
from __future__ import annotations

import argparse

import numpy as np

try:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
except ImportError:
    plt = None

THRESHOLD = 4.5


def welch_t(traces: np.ndarray, labels: np.ndarray) -> np.ndarray:
    fixed = traces[labels == 0]
    rand = traces[labels == 1]
    mf, mr = fixed.mean(0), rand.mean(0)
    vf, vr = fixed.var(0, ddof=1), rand.var(0, ddof=1)
    nf, nr = len(fixed), len(rand)
    denom = np.sqrt(vf / nf + vr / nr)
    denom[denom == 0] = np.nan
    return (mf - mr) / denom


def summarize(t: np.ndarray, label: str) -> dict:
    peak = float(np.nanmax(np.abs(t)))
    leaking = bool(peak > THRESHOLD)
    n_over = int(np.sum(np.abs(t) > THRESHOLD))
    print(f"[{label}] peak |t| = {peak:.2f}  | samples over 4.5 = {n_over}  | "
          f"verdict = {'LEAKS' if leaking else 'no first-order leakage detected'}")
    return {"label": label, "peak": peak, "leaking": leaking, "n_over": n_over}


def load(path):
    d = np.load(path)
    return d["traces"], d["labels"], str(d.get("variant", "unknown"))


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("safe_npz", help="capture from the constant-time firmware")
    ap.add_argument("--leaky-npz", help="capture from the leaky-control firmware")
    ap.add_argument("--plot", default="results/figures/tvla.png")
    cfg = ap.parse_args()

    results = []

    tr_s, lab_s, var_s = load(cfg.safe_npz)
    t_safe = welch_t(tr_s, lab_s)
    results.append(summarize(t_safe, f"safe ({var_s})"))

    t_leaky = None
    if cfg.leaky_npz:
        tr_l, lab_l, var_l = load(cfg.leaky_npz)
        t_leaky = welch_t(tr_l, lab_l)
        r = summarize(t_leaky, f"leaky-control ({var_l})")
        results.append(r)
        if not r["leaking"]:
            print("\nWARNING: leaky control did NOT trip the threshold. The "
                  "measurement chain is suspect; do not trust the safe result "
                  "until the control leaks as expected.")

    if plt is not None:
        n_rows = 2 if t_leaky is not None else 1
        fig, axes = plt.subplots(n_rows, 1, figsize=(8, 3 * n_rows), squeeze=False)
        axes[0][0].plot(t_safe, lw=0.6)
        axes[0][0].axhline(THRESHOLD, color="r", ls="--", lw=0.8)
        axes[0][0].axhline(-THRESHOLD, color="r", ls="--", lw=0.8)
        axes[0][0].set_title("Constant-time DUT (RustGuard ct_eq)")
        axes[0][0].set_ylabel("t-statistic")
        if t_leaky is not None:
            axes[1][0].plot(t_leaky, lw=0.6, color="C3")
            axes[1][0].axhline(THRESHOLD, color="r", ls="--", lw=0.8)
            axes[1][0].axhline(-THRESHOLD, color="r", ls="--", lw=0.8)
            axes[1][0].set_title("Leaky positive control (early-return compare)")
            axes[1][0].set_ylabel("t-statistic")
        axes[-1][0].set_xlabel("sample index")
        fig.tight_layout()
        import os
        os.makedirs(os.path.dirname(cfg.plot) or ".", exist_ok=True)
        fig.savefig(cfg.plot, dpi=150)
        print(f"figure -> {cfg.plot}")


if __name__ == "__main__":
    main()
