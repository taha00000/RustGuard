#!/usr/bin/env python3
"""dudect-style timing-leakage analysis for RustGuard (no oscilloscope needed).

Consumes the per-class cycle counts collected from the TM4C timing harness
(capture/collect_timing.py) and applies Welch's t-test to the *execution-time*
distributions, per Reparaz, Balasch & Verbauwhede, "Dude, is my code constant
time?" (DATE 2017). The instrument is the ARM core's own DWT cycle counter, so
the whole side-channel result is reproducible on a bare dev board.

  fixed class   : input held constant
  random class  : input drawn at random
  |t| > 4.5     : execution time depends on the input -> timing leakage
  |t| < 4.5     : classes statistically indistinguishable -> no timing leakage

Run on the safe (constant-time) capture and the leaky-control capture:
  * leaky control MUST exceed 4.5 (validates the method end to end)
  * safe DUT is the research result

  python analysis/dudect.py results/timing/safe.npz \
         --leaky results/timing/leaky.npz --plot results/figures/timing.png
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

try:
    from figutil import watermark
except ImportError:
    def watermark(_fig, _text):
        pass

THRESHOLD = 4.5


def welch_scalar(fixed: np.ndarray, rand: np.ndarray) -> float:
    """Welch's t between two 1-D samples of cycle counts.

    Cycle counts can be (near-)deterministic on an MCU, so guard the degenerate
    zero-variance case: identical constants -> t=0 (no leak); different constants
    -> infinite separation (a definite, if trivial, leak)."""
    mf, mr = float(fixed.mean()), float(rand.mean())
    vf, vr = float(fixed.var(ddof=1)), float(rand.var(ddof=1))
    denom = np.sqrt(vf / len(fixed) + vr / len(rand))
    if denom == 0.0:
        return 0.0 if mf == mr else float("inf")
    return (mf - mr) / denom


def load(path):
    d = np.load(path)
    cyc = d["cycles"].astype(np.float64)
    lab = d["labels"]
    return cyc, lab, str(d.get("variant", "?")), str(d.get("experiment", "?"))


def analyze(path, label_prefix):
    cyc, lab, variant, experiment = load(path)
    fixed, rand = cyc[lab == 0], cyc[lab == 1]
    t = welch_scalar(fixed, rand)
    leaking = abs(t) > THRESHOLD
    name = f"{label_prefix} ({variant}/{experiment})"
    t_disp = "inf" if np.isinf(t) else f"{t:.2f}"
    print(f"[{name}] |t| = {t_disp}  | fixed mean={fixed.mean():.1f}c "
          f"random mean={rand.mean():.1f}c  | "
          f"verdict = {'LEAKS' if leaking else 'no timing leakage detected'}")
    return {"name": name, "t": t, "leaking": leaking,
            "fixed": fixed, "rand": rand}


def _hist(ax, r, title):
    lo = int(min(r["fixed"].min(), r["rand"].min()))
    hi = int(max(r["fixed"].max(), r["rand"].max()))
    bins = np.linspace(lo - 1, hi + 1, max(20, min(80, hi - lo + 3)))
    ax.hist(r["fixed"], bins=bins, alpha=0.6, label="fixed input", color="C0")
    ax.hist(r["rand"], bins=bins, alpha=0.6, label="random input", color="C3")
    t_disp = "inf" if np.isinf(r["t"]) else f"{r['t']:.1f}"
    ax.set_title(f"{title}   |t|={t_disp}")
    ax.set_xlabel("cycles")
    ax.set_ylabel("count")
    ax.legend()


def make_dudect_figure(safe_npz, leaky_npz=None,
                       plot="results/figures/timing.png", watermark_text=None):
    results = [analyze(safe_npz, "safe")]
    if leaky_npz:
        r = analyze(leaky_npz, "leaky-control")
        results.append(r)
        if not r["leaking"]:
            print("\nWARNING: leaky control did NOT trip the threshold. The "
                  "harness or sample count is suspect; do not trust the safe "
                  "result until the control leaks as expected.")

    if plt is not None:
        n = len(results)
        fig, axes = plt.subplots(n, 1, figsize=(8, 3.2 * n), squeeze=False)
        titles = ["Constant-time DUT (subtle::ct_eq)",
                  "Leaky positive control (early-return compare)"]
        for i, r in enumerate(results):
            _hist(axes[i][0], r, titles[i] if i < len(titles) else r["name"])
        watermark(fig, watermark_text)
        fig.tight_layout()
        import os
        os.makedirs(os.path.dirname(plot) or ".", exist_ok=True)
        fig.savefig(plot, dpi=150)
        plt.close(fig)
        print(f"figure -> {plot}")
    return results


def _t_curve(cyc, lab, points=25):
    """|t| as a function of the number of traces used — the standard dudect
    convergence check. A real leak's |t| grows past 4.5 and keeps climbing;
    constant-time code stays flat and low no matter how many traces you add."""
    fixed, rand = cyc[lab == 0], cyc[lab == 1]
    m = min(len(fixed), len(rand))
    ns = np.unique(np.linspace(50, m, points).astype(int))
    xs, ys = [], []
    for n in ns:
        t = welch_scalar(fixed[:n], rand[:n])
        xs.append(2 * n)  # total traces (fixed + random)
        ys.append(min(abs(t), 1e3) if np.isfinite(t) else 1e3)
    return np.array(xs), np.array(ys)


def make_convergence_figure(safe_npz, leaky_npz=None,
                            plot="results/figures/timing_convergence.png",
                            watermark_text=None):
    """|t| vs number of traces, for the constant-time DUT and the leaky control."""
    if plt is None:
        return plot
    fig, ax = plt.subplots(figsize=(6.5, 4.2))
    cyc, lab, _v, _e = load(safe_npz)
    xs, ys = _t_curve(cyc, lab)
    ax.plot(xs, ys, color="C0", label="constant-time DUT")
    if leaky_npz:
        lc, ll, _v2, _e2 = load(leaky_npz)
        lx, ly = _t_curve(lc, ll)
        ax.plot(lx, ly, color="C3", label="leaky control")
    ax.axhline(THRESHOLD, color="r", ls="--", lw=0.8, label=f"threshold {THRESHOLD}")
    ax.set_xlabel("traces")
    ax.set_ylabel("|t|")
    ax.set_yscale("log")
    ax.set_title("Timing-leakage t-statistic vs number of traces")
    ax.grid(True, which="both", ls=":", alpha=0.5)
    ax.legend()
    watermark(fig, watermark_text)
    fig.tight_layout()
    import os
    os.makedirs(os.path.dirname(plot) or ".", exist_ok=True)
    fig.savefig(plot, dpi=150)
    plt.close(fig)
    print(f"figure -> {plot}")
    return plot


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("safe_npz", help="timing capture from the constant-time firmware")
    ap.add_argument("--leaky", dest="leaky_npz",
                    help="timing capture from the --features leaky firmware")
    ap.add_argument("--plot", default="results/figures/timing.png")
    ap.add_argument("--watermark", default=None)
    a = ap.parse_args()
    make_dudect_figure(a.safe_npz, a.leaky_npz, a.plot, a.watermark)


if __name__ == "__main__":
    main()
