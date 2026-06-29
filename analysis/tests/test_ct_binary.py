"""Tests for the binary-level constant-time analyzer (objdump census)."""
from ct_binary import census_disasm, demangle, differential

# A controlled synthetic disassembly in rust-objdump --no-show-raw-insn format:
# a constant-time decrypt with 2 conditional branches (public loops) and a
# variable-time decrypt with 5 (the extra 3 = the secret-dependent early return).
DISASM = """\
00000000 <_ZN14rustguard_core18ascon_aead_decrypt17h0000000000000000E>:
       0: 	push	{r4, lr}
       4: 	cbz	r0, 0x10 <x>
       8: 	add	r0, r1
       c: 	bne	0x4 <y>
      10: 	it	eq
      12: 	bx	lr

00000020 <_ZN14rustguard_core31ascon_aead_decrypt_variabletime17h1111111111111111E>:
      20: 	push	{r4, lr}
      24: 	beq	0x40 <a>
      28: 	bne	0x40 <b>
      2c: 	cbnz	r2, 0x40 <c>
      30: 	cbz	r3, 0x40 <d>
      34: 	blt	0x40 <e>
      38: 	udiv	r0, r1, r2
      3c: 	bx	lr

00000050 <_ZN14rustguard_core18ascon_aead_encrypt17h2222222222222222E>:
      50: 	push	{r4, lr}
      54: 	bl	0x100 <something>
      58: 	bx	lr
"""


def test_demangle():
    assert demangle("_ZN14rustguard_core18ascon_aead_decrypt17habc0000000000000E") \
        == "rustguard_core::ascon_aead_decrypt"
    assert demangle("probe_decrypt_ct") == "probe_decrypt_ct"


def test_census_counts():
    c = census_disasm(DISASM)
    ct = c["rustguard_core::ascon_aead_decrypt"]
    var = c["rustguard_core::ascon_aead_decrypt_variabletime"]
    enc = c["rustguard_core::ascon_aead_encrypt"]
    assert ct["cond"] == 2 and ct["it"] == 1
    assert var["cond"] == 5 and var["div"] == 1
    assert enc["cond"] == 0  # bl is a call, not a conditional branch


def test_differential_detects_leak():
    ct, var, delta = differential(census_disasm(DISASM))
    assert ct == 2 and var == 5 and delta == 3


def test_unconditional_branch_not_counted():
    # `b` and `bl`/`bx` are unconditional / calls and must not count as cond
    d = """\
00000000 <_ZN3foo3barE>:
       0: 	b	0x10 <z>
       4: 	bl	0x20 <w>
       8: 	bx	lr
"""
    assert census_disasm(d)["foo::bar"]["cond"] == 0
