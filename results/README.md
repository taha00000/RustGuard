# results/

Intentionally empty in version control. Timing captures (`timing/*.npz`), parsed
perf CSVs, and figures (`figures/*.png`) are produced on the hardware bench by the
collection and analysis scripts. The repository ships no measured numbers — every
reported result is generated from a real capture run on the TM4C. See
docs/experiment_runbook.md.

The synthetic, watermarked output of `analysis/selftest.py` lands in `_demo/`
(gitignored) and is never a result.
