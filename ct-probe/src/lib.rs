//! Symbol probes for binary-level constant-time analysis.
//!
//! Each primitive is exported as a distinct, non-inlined symbol so that
//! `analysis/ct_binary.py` can disassemble it from the compiled thumbv7em
//! object and census its control flow. The constant-time decrypt and the
//! variable-time (leaky) decrypt are both present, which lets the analyzer take
//! the safe-vs-leaky *differential* — the extra, secret-dependent branch that
//! the leaky tag compare introduces is exactly what should NOT appear in the
//! constant-time build.
//!
//! This crate is analysis scaffolding, not production code.

#![no_std]

use rustguard_core::{
    ascon_aead_decrypt, ascon_aead_decrypt_variabletime, ascon_aead_encrypt, ascon_p, State,
};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

const NONCE: [u8; 16] = [0; 16];
const AD: [u8; 0] = [];

/// Constant-time AEAD decrypt (uses `subtle::ct_eq` for the tag check).
#[no_mangle]
#[inline(never)]
pub fn probe_decrypt_ct(
    key: &[u8; 16],
    ct: &[u8; 16],
    tag: &[u8; 16],
    out: &mut [u8; 16],
) -> bool {
    ascon_aead_decrypt(key, &NONCE, &AD, ct, out, tag)
}

/// Variable-time AEAD decrypt (early-return byte compare) — the leaky control.
#[no_mangle]
#[inline(never)]
pub fn probe_decrypt_var(
    key: &[u8; 16],
    ct: &[u8; 16],
    tag: &[u8; 16],
    out: &mut [u8; 16],
) -> bool {
    ascon_aead_decrypt_variabletime(key, &NONCE, &AD, ct, out, tag)
}

/// AEAD encrypt — no secret-dependent control flow is expected at any -O level.
#[no_mangle]
#[inline(never)]
pub fn probe_encrypt(key: &[u8; 16], pt: &[u8; 16], ct: &mut [u8; 16], tag: &mut [u8; 16]) {
    ascon_aead_encrypt(key, &NONCE, &AD, pt, ct, tag);
}

/// The permutation: a baseline whose only branches are the public round loop.
#[no_mangle]
#[inline(never)]
pub fn probe_permutation(state: &mut State, rounds: usize) {
    ascon_p(state, rounds);
}
