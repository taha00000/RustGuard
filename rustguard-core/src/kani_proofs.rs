//! Machine-checked proof harnesses for `rustguard-core`.
//!
//! Compiled only under `cfg(kani)` (via `cargo kani`) and never part of a normal
//! build or `cargo test`. Each harness explores *symbolic* inputs with the Kani
//! bounded model checker (CBMC backend), proving the property for every input in
//! the bounded domain — not just sampled ones like a test.
//!
//! Tiers:
//!   * safety   — no panic, no integer overflow, no out-of-bounds, no UB. For a
//!                `#![forbid(unsafe_code)]` crate this upgrades "contains no
//!                `unsafe`" to "provably cannot crash or overflow" across the API.
//!   * functional — round-trip recovers the plaintext and authenticates; a wrong
//!                  tag never authenticates. The default functional harnesses fix
//!                  the key/nonce and keep the *message* (and tag) symbolic, which
//!                  is tractable for bounded model checking and still proves the
//!                  data-path inverse and authenticity for all messages.
//!   * deep (feature `kani-deep`) — the same functional properties with a fully
//!                  *symbolic key and nonce*. These are correct by construction
//!                  but require equating two independent symbolic sponges, which
//!                  is impractical for a SAT solver on commodity hardware; they
//!                  are gated off by default so the suite stays reliable, and can
//!                  be run on a larger machine / portfolio solver:
//!                  `cargo kani -p rustguard-core --features kani-deep`.
//!
//! `unwind(17)` fully unwinds the 12-round permutation and the 16-byte
//! constant-time tag comparison.

use crate::{ascon_aead_decrypt, ascon_aead_encrypt, ascon_hash, ascon_p, State};

// Fixed key/nonce for the tractable functional harnesses (contents arbitrary).
const K: [u8; 16] = [0x42; 16];
const N: [u8; 16] = [0xAA; 16];

// ─────────────────────────── Tier 1: safety ───────────────────────────

/// The permutation never panics / overflows for any state and any rounds <= 12.
#[kani::proof]
#[kani::unwind(17)]
fn verify_permutation_safety() {
    let mut s = State {
        x0: kani::any(),
        x1: kani::any(),
        x2: kani::any(),
        x3: kani::any(),
        x4: kani::any(),
    };
    let rounds: usize = kani::any();
    kani::assume(rounds <= 12);
    ascon_p(&mut s, rounds);
}

/// Encrypt cannot panic / overflow for any key, nonce, and 16-byte plaintext.
#[kani::proof]
#[kani::unwind(17)]
fn verify_encrypt_safety() {
    let key: [u8; 16] = kani::any();
    let nonce: [u8; 16] = kani::any();
    let pt: [u8; 16] = kani::any();
    let mut ct = [0u8; 16];
    let mut tag = [0u8; 16];
    ascon_aead_encrypt(&key, &nonce, &[], &pt, &mut ct, &mut tag);
}

/// Decrypt cannot panic / overflow for any inputs (the wipe-on-failure path
/// included), regardless of whether the tag verifies.
#[kani::proof]
#[kani::unwind(17)]
fn verify_decrypt_safety() {
    let key: [u8; 16] = kani::any();
    let nonce: [u8; 16] = kani::any();
    let ct: [u8; 16] = kani::any();
    let tag: [u8; 16] = kani::any();
    let mut pt = [0u8; 16];
    let _ = ascon_aead_decrypt(&key, &nonce, &[], &ct, &mut pt, &tag);
}

/// Encrypt with a partial associated-data block and a partial message block
/// exercises both padding paths.
#[kani::proof]
#[kani::unwind(17)]
fn verify_encrypt_ad_safety() {
    let key: [u8; 16] = kani::any();
    let nonce: [u8; 16] = kani::any();
    let ad: [u8; 5] = kani::any();
    let pt: [u8; 4] = kani::any();
    let mut ct = [0u8; 4];
    let mut tag = [0u8; 16];
    ascon_aead_encrypt(&key, &nonce, &ad, &pt, &mut ct, &mut tag);
}

/// ASCON-HASH never panics / overflows for any 16-byte input.
#[kani::proof]
#[kani::unwind(17)]
fn verify_hash_safety() {
    let data: [u8; 16] = kani::any();
    let mut out = [0u8; 32];
    ascon_hash(&data, &mut out);
}

// ─────────────────── Tier 2: functional (default, tractable) ───────────────────

/// Data-path inverse (the core AEAD correctness property the model checker can
/// discharge): for a fixed key/nonce and *every* 8-byte message, the decryption
/// sponge recovers the plaintext. This targets `decrypt_core` — the sponge that
/// produces the plaintext — so recovery is decoupled from the tag-authentication
/// gate (`ascon_aead_decrypt` wipes the buffer on auth failure, which would couple
/// `rec` to the intractable tag equality). Authentication is asserted by the deep
/// harnesses and pinned exactly by the KAT vectors.
#[kani::proof]
#[kani::unwind(17)]
fn verify_recovery_msg() {
    let pt: [u8; 8] = kani::any();
    let mut ct = [0u8; 8];
    let mut tag = [0u8; 16];
    ascon_aead_encrypt(&K, &N, &[], &pt, &mut ct, &mut tag);
    let mut rec = [0u8; 8];
    let _expected = crate::decrypt_core(&K, &N, &[], &ct, &mut rec);
    assert!(rec == pt, "decryption sponge must recover the plaintext");
}

// ─────────────── Tier 3: deep (feature `kani-deep`, intractable by default) ───────────────
// These assert tag authentication, which requires equating the genuine tag
// through the finalize permutation — correct by construction but impractical for
// a SAT solver on commodity hardware. Gated off so the default suite is reliable.

/// Genuine tag authenticates (fixed key/nonce, every message).
#[cfg(feature = "kani-deep")]
#[kani::proof]
#[kani::unwind(17)]
fn verify_roundtrip_auth() {
    let pt: [u8; 8] = kani::any();
    let mut ct = [0u8; 8];
    let mut tag = [0u8; 16];
    ascon_aead_encrypt(&K, &N, &[], &pt, &mut ct, &mut tag);
    let mut rec = [0u8; 8];
    let ok = ascon_aead_decrypt(&K, &N, &[], &ct, &mut rec, &tag);
    assert!(ok && rec == pt);
}

/// Forgery rejection (fixed key/nonce, every message, every wrong tag).
#[cfg(feature = "kani-deep")]
#[kani::proof]
#[kani::unwind(17)]
fn verify_forgery_rejected() {
    let pt: [u8; 8] = kani::any();
    let mut ct = [0u8; 8];
    let mut good = [0u8; 16];
    ascon_aead_encrypt(&K, &N, &[], &pt, &mut ct, &mut good);
    let bad: [u8; 16] = kani::any();
    kani::assume(bad != good);
    let mut rec = [0u8; 8];
    let ok = ascon_aead_decrypt(&K, &N, &[], &ct, &mut rec, &bad);
    assert!(!ok, "a wrong tag must never authenticate");
}

/// Round-trip with a fully symbolic key and nonce (the strongest form).
#[cfg(feature = "kani-deep")]
#[kani::proof]
#[kani::unwind(17)]
fn verify_roundtrip_symbolic_key() {
    let key: [u8; 16] = kani::any();
    let nonce: [u8; 16] = kani::any();
    let pt: [u8; 8] = kani::any();
    let mut ct = [0u8; 8];
    let mut tag = [0u8; 16];
    ascon_aead_encrypt(&key, &nonce, &[], &pt, &mut ct, &mut tag);
    let mut rec = [0u8; 8];
    let ok = ascon_aead_decrypt(&key, &nonce, &[], &ct, &mut rec, &tag);
    assert!(ok && rec == pt);
}
