# C / assembly baseline (pqm4 + ASCON reference)

This directory integrates the established C and hand-optimized assembly ASCON-128
implementations so the paper can compare Rust against them on identical hardware
under identical measurement conditions. Three baselines are wired in:

1. **ASCON reference C** (`ascon/ascon-c`) — portable `ref` and `opt64` variants.
2. **pqm4 Cortex-M assembly** — the `armv7m` hand-scheduled implementation, the
   fastest published M4 number (~7.9 cyc/B at 168 MHz).
3. (optional) **RustCrypto `ascon-aead`** — for a Rust-vs-Rust cross-check.

These are pulled as git submodules by `setup.sh` rather than vendored, so the
repo stays clean and licensing stays with upstream.

## Why this matters for the paper

The Reframe-1 claim is about the *cost of memory safety* and *whether constant-
time survives compilation*. Both require a same-board, same-toolchain baseline:
- Performance: Rust cyc/B vs C cyc/B vs asm cyc/B on the TM4C.
- Side-channel: Rust ct_eq vs C constant-time-compare under identical TVLA.

## Setup

```sh
./setup.sh          # clones ascon-c and pqm4 as submodules
make tm4c-bench     # builds the C baseline benchmark firmware for the TM4C
make stm32-tvla     # builds the C TVLA target for the CW308/STM32F3
```

> TODO(hardware): `setup.sh` and the Makefiles assume the arm-none-eabi-gcc
> toolchain is installed and the targets above. Confirm toolchain paths in
> `toolchain.mk` before first build.
