# Paper draft

`main.tex` is the working draft. It compiles with a stock LaTeX install:

```sh
cd paper
pdflatex main && bibtex main && pdflatex main && pdflatex main
```

To switch to the TCHES submission class, replace the `\documentclass` line with

```latex
\documentclass[journal=tches,submission,spdx=false]{iacrtrans}
```

and drop the `geometry` line. Everything else is portable.

## Before this is submitted anywhere

- **Every citation in `refs.bib` is written from memory and marked `VERIFY`.**
  Check each one against the actual publication — authors, venue, year, and
  above all that the paper supports the claim it is cited for. This is the
  single most likely source of an embarrassing error in the draft.
- `\todo{...}` marks the gaps: Section 2's background citations, the related-work
  section, and the Raspberry Pi results once that hardware is available.
- Numbers in the text are pulled from `results/tables/`. If the data is
  regenerated, re-check them rather than assuming they are unchanged.

## Where each number comes from

| Claim in the paper | Source |
|---|---|
| Cycle costs per primitive | `results/tables/leakage_matrix.md`, raw `.npz` |
| Zero-spread counts (108/130, 116/127) | `python analysis/resolution.py` |
| Detection floor (0.49 cycles) | `results/tables/detection_floor.md` |
| Optimizer erases the control | `results/tables/leakage_matrix.md`, rows `*-LEAKY-*` |
| Flash wait-state inflation | `tm4c_80MHz_*` vs `tm4c_O3_*`, `stm32_32MHz_*` vs `stm32_O3_*` |
| Static screening false positives | `results/tables/static_vs_measured.md` |
| Public-key cycle counts | `tm4c_O3_x25519-dalek_keyed.npz`, `tm4c_O3_p256-scalarmul_keyed.npz` |

## Figures

`results/figures/` holds the generated figures. `leakage_matrix.png` and
`leakage_matrix_keyed.png` are the ones the paper leans on; regenerate with
`python analysis/make_figures.py`.
