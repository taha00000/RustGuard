#!/usr/bin/env python3
"""Collect dudect-style timing samples for every primitive in the firmware registry.

Drives a board flashed with the multi-primitive timing harness
(`firmware-tm4c --features timing` or `firmware-stm32-timing`). One image carries
every probe, so a full ecosystem sweep is a single flash followed by one pass per
primitive — no reflashing between crates.

REQUIRES THE BOARD. It is a real serial driver, not a simulation. To exercise the
analysis without hardware, use analysis/selftest.py.

## Experiment design (per primitive)
Both classes present a *wrong* tag, so both reject and both execute the same
failure path; only the comparison differs:
  * fixed class  : the genuine tag with its last byte flipped -> matches a long
                   prefix, so an early-return compare runs nearly the full loop
  * random class : a uniformly random tag -> mismatches almost immediately
Constant-time compare => the classes are indistinguishable (|t| ~ 0).
Early-return compare  => the fixed class is measurably slower (|t| >> 4.5).
Classes are interleaved so slow drift cannot masquerade as leakage.

  # sweep every primitive on the board
  python capture/collect_timing.py --port COM20 --board tm4c --opt O3 \
         --n 3000 --outdir results/timing

  # a single primitive
  python capture/collect_timing.py --port COM20 --probe 3 --n 20000 \
         --out results/timing/aes-gcm.npz
"""
from __future__ import annotations

import argparse
import os
import re
import sys

import numpy as np

try:
    import serial  # pyserial
except ImportError:
    serial = None


def _readline(ser) -> str:
    return ser.readline().decode("ascii", "replace").strip()


def _wait_ready(ser, tries: int = 6) -> None:
    """Catch the boot banner if it is still coming.

    The board prints READY once at reset. When the port is opened after that —
    the normal case, since flashing resets the board before we connect — the
    banner is long gone and every read times out, so keep the budget small: the
    protocol is request/response and needs no handshake to work.
    """
    for _ in range(tries):
        if _readline(ser).endswith("READY"):
            return


def list_probes(ser):
    """Ask the firmware what it carries -> [(id, tag_len, name), ...]."""
    ser.reset_input_buffer()
    ser.write(b"l")
    out = []
    for _ in range(200):
        ln = _readline(ser)
        if ln == "endp":
            break
        m = re.match(r"^p (\d+) (\d+) (\S+)$", ln)
        if m:
            out.append((int(m.group(1)), int(m.group(2)), m.group(3)))
    return out


def select(ser, probe_id: int):
    """-> (tag_len, key_len). Older firmware replies with the tag length only."""
    ser.reset_input_buffer()
    ser.write(b"s" + f"{probe_id:02x}".encode())
    ln = _readline(ser)
    if not ln.startswith("ok"):
        raise RuntimeError(f"select {probe_id} failed: {ln!r}")
    parts = ln.split()
    return int(parts[1]), (int(parts[2]) if len(parts) > 2 else 16)


def correct_tag(ser) -> bytes:
    ser.reset_input_buffer()
    ser.write(b"g")
    ln = _readline(ser)
    if not ln.startswith("tag "):
        raise RuntimeError(f"expected 'tag <hex>', got {ln!r}")
    return bytes.fromhex(ln.split()[1])


def measure(ser, payload: bytes, cmd: bytes = b"v") -> int:
    ser.write(cmd + payload.hex().encode())
    ln = _readline(ser)
    if not ln.startswith("cyc "):
        raise RuntimeError(f"expected 'cyc <n>', got {ln!r}")
    return int(ln.split()[1])


def run_probe(ser, probe_id, tag_len, name, n, rng):
    """Interleaved fixed/random verification timing for one primitive."""
    select(ser, probe_id)
    good = correct_tag(ser)
    if len(good) != tag_len:
        raise RuntimeError(f"{name}: tag length {len(good)} != declared {tag_len}")
    # fixed class: genuine tag, last byte flipped -> long prefix match
    fixed = good[:-1] + bytes([good[-1] ^ 0xFF])

    cycles = np.empty(n, dtype=np.uint32)
    labels = np.empty(n, dtype=np.uint8)
    for i in range(n):
        is_random = (i % 2) == 1
        tag = bytes(rng.integers(0, 256, tag_len, dtype=np.uint8)) if is_random else fixed
        cycles[i] = measure(ser, tag)
        labels[i] = 1 if is_random else 0
    return cycles, labels


def run_probe_keyed(ser, probe_id, name, n, rng):
    """Classic dudect: fixed key vs random key, timing the primitive itself.

    The verify experiment holds the key constant and varies the tag, so it only
    exercises the final comparison. Here the *secret* varies, which is what puts
    the key schedule, block function and field arithmetic under test.
    """
    _tag_len, key_len = select(ser, probe_id)
    fixed = bytes([0x42] * key_len)

    cycles = np.empty(n, dtype=np.uint32)
    labels = np.empty(n, dtype=np.uint8)
    for i in range(n):
        is_random = (i % 2) == 1
        key = bytes(rng.integers(0, 256, key_len, dtype=np.uint8)) if is_random else fixed
        cycles[i] = measure(ser, key, cmd=b"k")
        labels[i] = 1 if is_random else 0
    return cycles, labels


def save(path, cycles, labels, probe, board, opt, experiment="verify", clock="default"):
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    np.savez_compressed(
        path,
        cycles=cycles,
        labels=labels,
        variant=probe,
        experiment=experiment,
        probe=probe,
        board=board,
        opt=opt,
        clock=clock,
    )


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--port", required=True, help="serial port, e.g. COM20 or /dev/ttyUSB0")
    ap.add_argument("--baud", type=int, default=115200)
    ap.add_argument("--n", type=int, default=3000, help="traces per primitive")
    ap.add_argument("--seed", type=int, default=0xC0FFEE)
    ap.add_argument("--board", default="tm4c", help="label recorded in the results")
    ap.add_argument("--opt", default="O3", help="optimization level label")
    ap.add_argument("--probe", type=int, help="run only this probe id")
    ap.add_argument("--out", help="output .npz (single-probe mode)")
    ap.add_argument("--outdir", default="results/timing", help="output dir (sweep mode)")
    ap.add_argument("--list", action="store_true", help="list probes and exit")
    ap.add_argument(
        "--clock",
        default="default",
        help="core-frequency label for the clock/flash-latency axis, e.g. 32MHz; "
        "'default' means the board's reset clock (16 MHz TM4C, 8 MHz STM32)",
    )
    ap.add_argument(
        "--experiment",
        choices=["verify", "keyed"],
        default="verify",
        help="verify = fixed key, varying tag (comparison path); "
        "keyed = fixed vs random key (the primitive's own core)",
    )
    cfg = ap.parse_args()

    if serial is None:
        sys.exit("pyserial not installed. `pip install pyserial`.")
    ser = serial.Serial(cfg.port, cfg.baud, timeout=3)
    _wait_ready(ser)

    probes = list_probes(ser)
    if not probes:
        sys.exit("firmware reported no probes — is the timing firmware flashed?")

    if cfg.list:
        for pid, tl, nm in probes:
            print(f"  {pid:>3}  tag={tl:<3} {nm}")
        ser.close()
        return

    rng = np.random.default_rng(cfg.seed)
    targets = [p for p in probes if cfg.probe is None or p[0] == cfg.probe]
    if not targets:
        sys.exit(f"probe id {cfg.probe} not present in this firmware")

    keyed = cfg.experiment == "keyed"
    for pid, tag_len, name in targets:
        if keyed:
            cycles, labels = run_probe_keyed(ser, pid, name, cfg.n, rng)
        else:
            cycles, labels = run_probe(ser, pid, tag_len, name, cfg.n, rng)
        if cfg.out and cfg.probe is not None:
            path = cfg.out
        else:
            # keyed captures get their own filenames so both experiments can
            # live side by side in one results directory
            suffix = "_keyed" if keyed else ""
            clk = "" if cfg.clock == "default" else f"_{cfg.clock}"
            path = os.path.join(
                cfg.outdir, f"{cfg.board}{clk}_{cfg.opt}_{name}{suffix}.npz"
            )
        save(path, cycles, labels, name, cfg.board, cfg.opt, cfg.experiment, cfg.clock)
        f_mean = cycles[labels == 0].mean()
        r_mean = cycles[labels == 1].mean()
        print(f"  {name:<26} fixed={f_mean:>9.1f}c random={r_mean:>9.1f}c -> {path}")

    ser.close()
    print(f"done: {len(targets)} primitive(s), {cfg.n} traces each")


if __name__ == "__main__":
    main()
