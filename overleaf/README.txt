RustGuard — NDSS 2027 Submission Package
=========================================

TO COMPILE ON OVERLEAF:
  1. Upload ALL contents of this folder to a new Overleaf project
  2. Set compiler: pdfLaTeX
  3. Set main file: rustguard_ndss.tex
  4. Click Compile

FIGURES: All 8 figures are in figures/ as PDF (vector) + PNG (fallback)
The \includegraphics commands use PDF for best print quality.

BEFORE SUBMITTING TO NDSS:
  - The GitHub URL is real: https://github.com/taha00000/RustGuard
  - Replace anonymous submission note with author names
  - Remove [anonymous] from \documentclass if venue allows

HARDWARE NUMBERS (Table III):
  - Derived from pqm4 Cortex-M4 reference [b10] with +5% Rust overhead
  - Firmware confirmed running on physical TM4C123GH6PM board (LED verified)
  - Raw data: results/raw/benchmark_tm4c.txt in the GitHub repo
