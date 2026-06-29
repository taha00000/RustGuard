"""Tests for the table generators and size parsing."""
import numpy as np

from figutil import parse_size_output, write_table
from tables import perf_cycle_table, timing_table


def test_parse_size_output():
    txt = ("   text\t   data\t    bss\t    dec\t    hex\tfilename\n"
           "  12000\t     16\t   1040\t  13056\t   3300\tfirmware-rust\n")
    d = parse_size_output(txt)
    assert len(d) == 1
    assert d[0]["name"] == "firmware-rust"
    assert d[0]["flash"] == 12016   # text + data
    assert d[0]["ram"] == 1056      # data + bss


def test_parse_size_skips_header_and_junk():
    assert parse_size_output("garbage\ntext data bss\n") == []


def test_write_table_md_and_tex(tmp_path):
    md = tmp_path / "t.md"
    tex = tmp_path / "t.tex"
    write_table(["a", "b"], [[1, 2], [3, 4]], str(md), str(tex),
                caption="cap", label="tab:x")
    md_txt = md.read_text(encoding="utf-8")
    assert "| a | b |" in md_txt and "| 1 | 2 |" in md_txt
    tex_txt = tex.read_text(encoding="utf-8")
    assert "\\begin{table}" in tex_txt and "\\caption{cap}" in tex_txt
    assert "1 & 2" in tex_txt


def test_perf_cycle_table(tmp_path):
    datasets = {
        "rust": [{"op": "ENC", "size": 16, "mean_cyc": 320, "cyc_per_byte": 20.0}],
        "cref": [{"op": "ENC", "size": 16, "mean_cyc": 256, "cyc_per_byte": 16.0}],
    }
    md = tmp_path / "perf.md"
    perf_cycle_table(datasets, str(md))
    txt = md.read_text(encoding="utf-8")
    assert "bytes" in txt and "16" in txt
    assert "1.25x" in txt  # 20.0 / 16.0 overhead vs best baseline


def test_timing_table(tmp_path):
    safe = tmp_path / "safe.npz"
    cyc = np.concatenate([np.full(1000, 4200), np.full(1000, 4200)]).astype(np.uint32)
    lab = np.concatenate([np.zeros(1000), np.ones(1000)]).astype(np.uint8)
    np.savez_compressed(safe, cycles=cyc, labels=lab, variant="safe", experiment="tagcompare")
    md = tmp_path / "timing.md"
    timing_table([str(safe)], str(md))
    txt = md.read_text(encoding="utf-8")
    assert "constant-time" in txt and "tagcompare" in txt
