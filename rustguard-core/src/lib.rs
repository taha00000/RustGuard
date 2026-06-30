//! rustguard-core — ASCON-128 AEAD + ASCON-HASH, `no_std`, memory-safe.
//!
//! Implements ASCON v1.2 per NIST IR 8454. The AEAD permutation, S-box, and
//! linear diffusion are branchless on secret data. Key material is zeroized on
//! drop via the `zeroize` crate. Tag comparison is constant-time via `subtle`.
//!
//! ## Why this crate exists (Reframe-1 research framing)
//!
//! This is the *device under test* for a study of whether Rust's source-level
//! constant-time guarantees survive compilation to embedded silicon. The same
//! AEAD is exposed in two forms:
//!
//! * [`ascon_aead_decrypt`] — the constant-time reference path (uses
//!   `subtle::ConstantTimeEq` for tag verification).
//! * [`ascon_aead_decrypt_variabletime`] — an intentionally *leaky* tag check
//!   (early-return byte compare) used only as a positive control in TVLA
//!   experiments, gated behind the `tvla-leaky-control` feature.
//!
//! The leaky path is NEVER compiled into production builds. It exists so the
//! side-channel experiment has a known-leaking baseline to validate the TVLA
//! pipeline against (a TVLA setup that cannot detect the leaky version is
//! broken and its "no leakage" result on the safe version would be meaningless).

#![no_std]
#![forbid(unsafe_code)]

use subtle::ConstantTimeEq;
#[cfg(not(kani))]
use zeroize::Zeroize;

// ── State ─────────────────────────────────────────────────────────────────────

/// The 320-bit ASCON permutation state as five 64-bit words.
/// `Zeroize` + `#[zeroize(drop)]` guarantees key material is erased on drop.
/// (Under `cfg(kani)` the zeroize-on-drop is dropped, because Kani cannot model
/// zeroize's inline-asm optimization barrier; this does not affect the functional
/// or safety properties being proven.)
#[derive(Clone)]
#[cfg_attr(not(kani), derive(Zeroize), zeroize(drop))]
pub struct State {
    pub x0: u64,
    pub x1: u64,
    pub x2: u64,
    pub x3: u64,
    pub x4: u64,
}

// ── Round constants (ASCON v1.2 spec Table 2) ─────────────────────────────────
const ROUND_CONSTANTS: [u64; 12] = [
    0x0000_0000_0000_00f0,
    0x0000_0000_0000_00e1,
    0x0000_0000_0000_00d2,
    0x0000_0000_0000_00c3,
    0x0000_0000_0000_00b4,
    0x0000_0000_0000_00a5,
    0x0000_0000_0000_0096,
    0x0000_0000_0000_0087,
    0x0000_0000_0000_0078,
    0x0000_0000_0000_0069,
    0x0000_0000_0000_005a,
    0x0000_0000_0000_004b,
];

/// ASCON-128 IV = key_len(128) ‖ rate(64) ‖ pa(12) ‖ pb(6) ‖ 0^32.
const ASCON128_IV: u64 = 0x8040_0c06_0000_0000;

/// ASCON-HASH IV (spec §2.5).
const ASCON_HASH_IV: [u64; 5] = [
    0xee93_98aa_db67_f03d,
    0x8bb2_1831_c60f_1002,
    0xb48a_92db_98d5_da62,
    0x4318_9921_b8f8_e3e8,
    0x348f_a5c9_d525_e140,
];

// ── S-box (branchless, bitsliced across the 5-word state) ────────────────────

/// The ASCON S-box χ applied to the full 320-bit state in bitsliced form.
/// No branches, no table lookups — constant-time on secret data.
#[inline(always)]
fn ascon_sbox(s: &mut State) {
    s.x0 ^= s.x4;
    s.x4 ^= s.x3;
    s.x2 ^= s.x1;
    let t0 = s.x0;
    let t1 = s.x1;
    let t2 = s.x2;
    let t3 = s.x3;
    let t4 = s.x4;
    s.x0 = t0 ^ (!t1 & t2);
    s.x1 = t1 ^ (!t2 & t3);
    s.x2 = t2 ^ (!t3 & t4);
    s.x3 = t3 ^ (!t4 & t0);
    s.x4 = t4 ^ (!t0 & t1);
    s.x1 ^= s.x0;
    s.x0 ^= s.x4;
    s.x3 ^= s.x2;
    s.x2 = !s.x2;
}

/// Linear diffusion Σ per spec §2.2, step 3.
#[inline(always)]
fn ascon_diffusion(s: &mut State) {
    s.x0 ^= s.x0.rotate_right(19) ^ s.x0.rotate_right(28);
    s.x1 ^= s.x1.rotate_right(61) ^ s.x1.rotate_right(39);
    s.x2 ^= s.x2.rotate_right(1) ^ s.x2.rotate_right(6);
    s.x3 ^= s.x3.rotate_right(10) ^ s.x3.rotate_right(17);
    s.x4 ^= s.x4.rotate_right(7) ^ s.x4.rotate_right(41);
}

/// One ASCON round: AddConstants → SubBytes → LinearDiffusion.
#[inline(always)]
pub fn ascon_round(s: &mut State, rc: u64) {
    s.x2 ^= rc;
    ascon_sbox(s);
    ascon_diffusion(s);
}

/// Apply p^a (the full permutation with `rounds` rounds).
/// rounds = 12 for initialization / finalization; 6 for data processing.
pub fn ascon_p(s: &mut State, rounds: usize) {
    debug_assert!(rounds <= 12, "ASCON permutation: rounds must be <= 12");
    let start = 12 - rounds;
    for i in start..12 {
        ascon_round(s, ROUND_CONSTANTS[i]);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

#[inline(always)]
fn load64be(src: &[u8]) -> u64 {
    u64::from_be_bytes(src[..8].try_into().unwrap())
}

#[inline(always)]
fn store64be(dst: &mut [u8], v: u64) {
    dst[..8].copy_from_slice(&v.to_be_bytes());
}

/// Zero a buffer. Uses `zeroize` in normal builds; under Kani (which cannot model
/// zeroize's inline-asm optimization barrier) it uses a plain, semantically
/// identical loop so the cipher logic can still be model-checked. Production
/// builds are unaffected.
#[inline(always)]
fn wipe(buf: &mut [u8]) {
    #[cfg(not(kani))]
    buf.zeroize();
    #[cfg(kani)]
    for b in buf.iter_mut() {
        *b = 0;
    }
}

// ── ASCON-128 AEAD Encrypt ────────────────────────────────────────────────────

/// ASCON-128 authenticated encryption.
///
/// # Panics
/// Panics if `ciphertext.len() != plaintext.len()`.
pub fn ascon_aead_encrypt(
    key: &[u8; 16],
    nonce: &[u8; 16],
    assoc_data: &[u8],
    plaintext: &[u8],
    ciphertext: &mut [u8],
    tag: &mut [u8; 16],
) {
    assert_eq!(
        ciphertext.len(),
        plaintext.len(),
        "ciphertext buffer must equal plaintext length"
    );

    let mut s = State {
        x0: ASCON128_IV,
        x1: load64be(&key[0..8]),
        x2: load64be(&key[8..16]),
        x3: load64be(&nonce[0..8]),
        x4: load64be(&nonce[8..16]),
    };
    ascon_p(&mut s, 12);
    s.x3 ^= load64be(&key[0..8]);
    s.x4 ^= load64be(&key[8..16]);

    if !assoc_data.is_empty() {
        let mut chunks = assoc_data.chunks_exact(8);
        for chunk in chunks.by_ref() {
            s.x0 ^= load64be(chunk);
            ascon_p(&mut s, 6);
        }
        let rem = chunks.remainder();
        let mut pad = [0u8; 8];
        pad[..rem.len()].copy_from_slice(rem);
        pad[rem.len()] = 0x80;
        s.x0 ^= u64::from_be_bytes(pad);
        ascon_p(&mut s, 6);
    }
    s.x4 ^= 1; // domain separation

    let mut pt_chunks = plaintext.chunks_exact(8);
    let mut ct_chunks = ciphertext.chunks_exact_mut(8);
    for (pt, ct) in pt_chunks.by_ref().zip(ct_chunks.by_ref()) {
        s.x0 ^= load64be(pt);
        store64be(ct, s.x0);
        ascon_p(&mut s, 6);
    }
    let pt_rem = pt_chunks.remainder();
    let ct_rem = ct_chunks.into_remainder();
    let mut pad = [0u8; 8];
    pad[..pt_rem.len()].copy_from_slice(pt_rem);
    pad[pt_rem.len()] = 0x80;
    s.x0 ^= u64::from_be_bytes(pad);
    let out = s.x0.to_be_bytes();
    ct_rem.copy_from_slice(&out[..ct_rem.len()]);

    s.x1 ^= load64be(&key[0..8]);
    s.x2 ^= load64be(&key[8..16]);
    ascon_p(&mut s, 12);
    s.x3 ^= load64be(&key[0..8]);
    s.x4 ^= load64be(&key[8..16]);
    store64be(&mut tag[0..8], s.x3);
    store64be(&mut tag[8..16], s.x4);
}

// ── ASCON-128 AEAD Decrypt (constant-time tag check) ─────────────────────────

/// ASCON-128 authenticated decryption with constant-time tag verification.
///
/// Returns `true` iff the tag is valid. On failure the `plaintext` buffer is
/// zeroized before returning (no partial-plaintext exposure).
pub fn ascon_aead_decrypt(
    key: &[u8; 16],
    nonce: &[u8; 16],
    assoc_data: &[u8],
    ciphertext: &[u8],
    plaintext: &mut [u8],
    tag: &[u8; 16],
) -> bool {
    let expected = decrypt_core(key, nonce, assoc_data, ciphertext, plaintext);
    // Constant-time comparison: no secret-dependent branch, no early return.
    let ok = bool::from(tag.ct_eq(&expected));
    if !ok {
        wipe(plaintext);
    }
    ok
}

/// **TVLA POSITIVE CONTROL ONLY — never use in production.**
///
/// Identical to [`ascon_aead_decrypt`] except the tag check is an intentionally
/// variable-time, early-returning byte comparison. This deliberately leaks tag
/// match progress through timing/power, giving the TVLA pipeline a known-leaking
/// reference. Gated behind `tvla-leaky-control` so it cannot be linked by
/// accident.
#[cfg(feature = "tvla-leaky-control")]
pub fn ascon_aead_decrypt_variabletime(
    key: &[u8; 16],
    nonce: &[u8; 16],
    assoc_data: &[u8],
    ciphertext: &[u8],
    plaintext: &mut [u8],
    tag: &[u8; 16],
) -> bool {
    let expected = decrypt_core(key, nonce, assoc_data, ciphertext, plaintext);
    let mut ok = true;
    // INTENTIONALLY LEAKY: early return on first mismatch.
    for i in 0..16 {
        if tag[i] != expected[i] {
            ok = false;
            break;
        }
    }
    if !ok {
        wipe(plaintext);
    }
    ok
}

/// Shared decryption core: runs the sponge and returns the *expected* tag.
/// The tag-comparison policy is applied by the caller.
fn decrypt_core(
    key: &[u8; 16],
    nonce: &[u8; 16],
    assoc_data: &[u8],
    ciphertext: &[u8],
    plaintext: &mut [u8],
) -> [u8; 16] {
    assert_eq!(
        plaintext.len(),
        ciphertext.len(),
        "plaintext buffer must equal ciphertext length"
    );

    let mut s = State {
        x0: ASCON128_IV,
        x1: load64be(&key[0..8]),
        x2: load64be(&key[8..16]),
        x3: load64be(&nonce[0..8]),
        x4: load64be(&nonce[8..16]),
    };
    ascon_p(&mut s, 12);
    s.x3 ^= load64be(&key[0..8]);
    s.x4 ^= load64be(&key[8..16]);

    if !assoc_data.is_empty() {
        let mut chunks = assoc_data.chunks_exact(8);
        for chunk in chunks.by_ref() {
            s.x0 ^= load64be(chunk);
            ascon_p(&mut s, 6);
        }
        let rem = chunks.remainder();
        let mut pad = [0u8; 8];
        pad[..rem.len()].copy_from_slice(rem);
        pad[rem.len()] = 0x80;
        s.x0 ^= u64::from_be_bytes(pad);
        ascon_p(&mut s, 6);
    }
    s.x4 ^= 1;

    let mut ct_chunks = ciphertext.chunks_exact(8);
    let mut pt_chunks = plaintext.chunks_exact_mut(8);
    for (ct, pt) in ct_chunks.by_ref().zip(pt_chunks.by_ref()) {
        let c = load64be(ct);
        let p = s.x0 ^ c;
        store64be(pt, p);
        s.x0 = c;
        ascon_p(&mut s, 6);
    }
    let ct_rem = ct_chunks.remainder();
    let pt_rem = pt_chunks.into_remainder();
    for i in 0..ct_rem.len() {
        let shift = 56 - 8 * i;
        pt_rem[i] = ct_rem[i] ^ (s.x0 >> shift) as u8;
        s.x0 = (s.x0 & !(0xFFu64 << shift)) | ((ct_rem[i] as u64) << shift);
    }
    s.x0 ^= 0x80u64 << (56 - 8 * ct_rem.len());

    s.x1 ^= load64be(&key[0..8]);
    s.x2 ^= load64be(&key[8..16]);
    ascon_p(&mut s, 12);
    s.x3 ^= load64be(&key[0..8]);
    s.x4 ^= load64be(&key[8..16]);

    let mut expected = [0u8; 16];
    store64be(&mut expected[0..8], s.x3);
    store64be(&mut expected[8..16], s.x4);
    expected
}

// ── ASCON-HASH ────────────────────────────────────────────────────────────────

/// ASCON-HASH: 256-bit digest. Rate = 64 bits, absorb/squeeze with p^12.
pub fn ascon_hash(data: &[u8], out: &mut [u8; 32]) {
    let mut s = State {
        x0: ASCON_HASH_IV[0],
        x1: ASCON_HASH_IV[1],
        x2: ASCON_HASH_IV[2],
        x3: ASCON_HASH_IV[3],
        x4: ASCON_HASH_IV[4],
    };

    let mut chunks = data.chunks_exact(8);
    for chunk in chunks.by_ref() {
        s.x0 ^= load64be(chunk);
        ascon_p(&mut s, 12);
    }
    let rem = chunks.remainder();
    let mut pad = [0u8; 8];
    pad[..rem.len()].copy_from_slice(rem);
    pad[rem.len()] = 0x80;
    s.x0 ^= u64::from_be_bytes(pad);
    ascon_p(&mut s, 12);

    store64be(&mut out[0..8], s.x0);
    ascon_p(&mut s, 12);
    store64be(&mut out[8..16], s.x0);
    ascon_p(&mut s, 12);
    store64be(&mut out[16..24], s.x0);
    ascon_p(&mut s, 12);
    store64be(&mut out[24..32], s.x0);
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AsconError {
    AuthenticationFailed,
    BufferTooSmall,
}

// Machine-checked proof harnesses (compiled only under `cargo kani`).
#[cfg(kani)]
mod kani_proofs;
