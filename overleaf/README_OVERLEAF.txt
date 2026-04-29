RustGuard — NDSS 2027 Submission
=================================

TO COMPILE ON OVERLEAF:
  1. Upload the entire contents of this folder to a new Overleaf project
  2. Set the main file to: rustguard_ndss.tex
  3. Set compiler to: pdfLaTeX
  4. Click Compile

FILES:
  rustguard_ndss.tex   - Main paper source (anonymous, NDSS format)
  ndss.cls             - NDSS conference class file
  figures/             - All 7 figures as PDF (vector, best quality)
                         and PNG (fallback if PDF import fails)

BEFORE SUBMISSION:
  - Replace anonymous repo URL with actual GitHub URL after deanonymization
  - Fill in hardware benchmark numbers from TM4C123 board execution
    (see Section VII of the paper and scripts/parse_hw_results.py)
  - Verify page count ≤ 13 (excluding Ethics, References, Appendices)

FIGURE FILENAMES (must match \includegraphics in .tex):
  figures/fig1_latency.pdf
  figures/fig2_throughput.pdf
  figures/fig3_permutation.pdf
  figures/fig4_breakdown.pdf
  figures/fig5_overhead.pdf
  figures/fig6_tests.pdf
  figures/fig7_codesize.pdf
