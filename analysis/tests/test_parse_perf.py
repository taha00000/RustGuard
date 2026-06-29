"""Tests for the TM4C UART perf parser."""
from parse_perf import parse

SAMPLE = """\
# RustGuard TM4C123 perf benchmark (real DWT)
# 16 MHz, opt-level=3, lto=true. 500 iters, 50 warmup
PERM p6 mean_cyc=420
PERM p12 mean_cyc=910
SECTION:ENCRYPT
ENC 8 mean_cyc=1600 cpb_x100=20000
ENC 16 mean_cyc=2400 cpb_x100=15000
SECTION:DECRYPT
DEC 8 mean_cyc=1650
SECTION:DONE
""".splitlines()


def test_perm_parsed():
    _rows, perm = parse(SAMPLE)
    assert perm == {"p6": 420, "p12": 910}


def test_enc_dec_rows():
    rows, _perm = parse(SAMPLE)
    enc = [r for r in rows if r["op"] == "ENC"]
    dec = [r for r in rows if r["op"] == "DEC"]
    assert len(enc) == 2 and len(dec) == 1
    assert enc[0] == {"op": "ENC", "size": 8, "mean_cyc": 1600, "cyc_per_byte": 200.0}
    assert dec[0]["size"] == 8 and dec[0]["mean_cyc"] == 1650


def test_cyc_per_byte_computed():
    rows, _ = parse(SAMPLE)
    enc16 = next(r for r in rows if r["op"] == "ENC" and r["size"] == 16)
    assert enc16["cyc_per_byte"] == 150.0  # 2400 / 16


def test_empty_input_yields_nothing():
    rows, perm = parse([])
    assert rows == [] and perm == {}
