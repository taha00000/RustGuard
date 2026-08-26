"""Tests for the ecosystem-scale census: demangling, crate attribution, triage."""
import numpy as np

from ct_binary import by_crate, census_disasm, crate_of, demangle, triage
from matrix import collect_cells


def test_demangle_legacy():
    assert demangle("_ZN14rustguard_core18ascon_aead_decrypt17habc0000000000000E") \
        == "rustguard_core::ascon_aead_decrypt"


def test_demangle_v0_extracts_crate_path():
    # Rust v0: Cs<disambiguator>_<len><crate> then length-prefixed segments
    out = demangle("_RNvNtCs7NhdfSRABLd_17compiler_builtins3mem6memcpy")
    assert out.startswith("compiler_builtins")
    assert "memcpy" in out


def test_demangle_unescapes_trait_impl():
    raw = "_$LT$chacha20poly1305..X$u20$as$u20$aead..AeadInPlace$GT$"
    assert demangle(raw) == "_<chacha20poly1305::X as aead::AeadInPlace>"


def test_crate_of_handles_leading_underscore_trait_symbol():
    # LLVM's real emission has a leading `_`; the implementing crate must still win
    sym = "_<chacha20poly1305::X as aead::AeadInPlace>::encrypt_in_place_detached"
    assert crate_of(sym) == "chacha20poly1305"


def test_crate_of_plain_and_trait_impl():
    assert crate_of("aes_gcm::foo::bar") == "aes_gcm"
    # the crate implemented *for*, not the trait's crate
    assert crate_of("<chacha20poly1305::X as aead::AeadInPlace>::m") == "chacha20poly1305"


def test_crate_of_probe_wrapper_maps_to_primitive():
    # LTO inlines crate code into the probe wrapper; it must be attributed back
    assert crate_of("probes::p_aesgcm::verify") == "aes-gcm"
    assert crate_of("probes::p_chachapoly::correct") == "chacha20poly1305"


def test_by_crate_drops_infrastructure_and_aggregates():
    d = """\
00000000 <_ZN7aes_gcm4someE>:
       0: \tcbz\tr0, 0x10 <x>
       4: \tbne\t0x4 <y>
00000020 <_ZN4core3fmt5writeE>:
      20: \tbeq\t0x40 <a>
"""
    agg = by_crate(census_disasm(d))
    assert "aes_gcm" in agg
    assert agg["aes_gcm"]["cond"] == 2
    assert "core" not in agg  # infrastructure filtered out


def test_triage_weights_division_heavily():
    d = """\
00000000 <_ZN6cratea1fE>:
       0: \tudiv\tr0, r1, r2
00000020 <_ZN6crateb1gE>:
      20: \tbeq\t0x40 <a>
      24: \tbne\t0x40 <b>
"""
    ranked = triage(census_disasm(d))
    # one udiv (weight 10) must outrank two plain branches (weight 1 each)
    assert ranked[0][1].startswith("cratea")


def test_collect_cells_reads_sweep_files(tmp_path):
    for name, leak in (("aes-gcm", False), ("leaky-thing", True)):
        cyc = np.full(400, 5000, dtype=np.uint32)
        lab = np.tile([0, 1], 200).astype(np.uint8)
        if leak:
            cyc = cyc.astype(np.float64)
            cyc[lab == 1] = 4900
            cyc = cyc.astype(np.uint32)
        np.savez_compressed(tmp_path / f"tm4c_O3_{name}.npz", cycles=cyc, labels=lab,
                            variant=name, experiment="verify", probe=name,
                            board="tm4c", opt="O3")
    cells, cols = collect_cells(str(tmp_path))
    assert cols == ["tm4c/O3"]
    assert cells["aes-gcm"]["tm4c/O3"][1] is False   # identical -> no leak
    assert cells["leaky-thing"]["tm4c/O3"][1] is True  # separated -> leaks
