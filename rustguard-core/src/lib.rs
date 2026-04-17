//! rustguard-core — ASCON-128 AEAD, ASCON-HASH, memory-safe, no_std
//!
//! Implements ASCON v1.2 per the NIST IR 8454 specification.
//! All permutation operations are branchless on secret data.
//! Key material is zeroized on drop via the `zeroize` crate.

#![no_std]
#![forbid(unsafe_code)]

use subtle::ConstantTimeEq;
use zeroize::Zeroize;

// ── State ─────────────────────────────────────────────────────────────────────

/// The 320-bit ASCON permutation state as five 64-bit words.
/// `Zeroize` + `#[zeroize(drop)]` guarantees key material is erased on drop.
#[derive(Clone, Zeroize)]
#[zeroize(drop)]
pub struct State {
    pub x0: u64,
    pub x1: u64,
    pub x2: u64,
    pub x3: u64,
    pub x4: u64,
}

// ── Round constants (ASCON v1.2 spec Table 2) ─────────────────────────────────
// rc[i] = 0xf0 - 0x0f*i for i in 0..12
const ROUND_CONSTANTS: [u64; 12] = [
    0x00000000000000f0,
    0x00000000000000e1,
    0x00000000000000d2,
    0x00000000000000c3,
    0x00000000000000b4,
    0x00000000000000a5,
    0x0000000000000096,
    0x0000000000000087,
    0x0000000000000078,
    0x0000000000000069,
    0x000000000000005a,
    0x000000000000004b,
];

// ── ASCON-128 IV (spec §2.1) ──────────────────────────────────────────────────
// IV = key_len(8) || rate(8) || pa(8) || pb(8) || 0^32
// = 128 || 64 || 12 || 6 || 0 => 0x80400c0600000000
const ASCON128_IV: u64 = 0x80400c0600000000;

// ── ASCON-HASH IV (spec §2.5) ─────────────────────────────────────────────────
const ASCON_HASH_IV: [u64; 5] = [
    0xee9398aadb67f03d,
    0x8bb21831c60f1002,
    0xb48a92db98d5da62,
    0x43189921b8f8e3e8,
    0x348fa5c9d525e140,
];

// ── S-box (branchless, bitsliced across the 5-word state) ────────────────────

/// The ASCON S-box χ applied to the full 320-bit state in bitsliced form.
/// No branches, no table lookups — constant-time on secret data.
#[inline(always)]
fn ascon_sbox(s: &mut State) {
    // Pre-XOR layer (spec §2.2, step 1 of χ)
    s.x0 ^= s.x4;
    s.x4 ^= s.x3;
    s.x2 ^= s.x1;
    // χ: y_i = x_i XOR (NOT x_{i+1} AND x_{i+2})
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
    // Post-XOR layer
    s.x1 ^= s.x0;
    s.x0 ^= s.x4;
    s.x3 ^= s.x2;
    s.x2 = !s.x2;
}

// ── Linear diffusion layer ────────────────────────────────────────────────────

/// Linear diffusion Σ per spec §2.2, step 3.
#[inline(always)]
fn ascon_diffusion(s: &mut State) {
    s.x0 ^= s.x0.rotate_right(19) ^ s.x0.rotate_right(28);
    s.x1 ^= s.x1.rotate_right(61) ^ s.x1.rotate_right(39);
    s.x2 ^= s.x2.rotate_right(1)  ^ s.x2.rotate_right(6);
    s.x3 ^= s.x3.rotate_right(10) ^ s.x3.rotate_right(17);
    s.x4 ^= s.x4.rotate_right(7)  ^ s.x4.rotate_right(41);
}

// ── Round function ────────────────────────────────────────────────────────────

/// One ASCON round: AddConstants → SubBytes → LinearDiffusion.
#[inline(always)]
pub fn ascon_round(s: &mut State, rc: u64) {
    s.x2 ^= rc;        // AddConstants
    ascon_sbox(s);     // SubBytes  (branchless)
    ascon_diffusion(s); // LinearDiffusion
}

/// Apply p^a (the full permutation with `rounds` rounds).
/// rounds = 12 for initialization / finalization; 6 for data processing.
pub fn ascon_p(s: &mut State, rounds: usize) {
    debug_assert!(rounds <= 12, "ASCON permutation: rounds must be ≤ 12");
    let start = 12 - rounds;
    for i in start..12 {
        ascon_round(s, ROUND_CONSTANTS[i]);
    }
}

// ── Helper: load/store big-endian u64 from byte slices ───────────────────────

#[inline(always)]
fn load64be(src: &[u8]) -> u64 {
    u64::from_be_bytes(src[..8].try_into().unwrap())
}

#[inline(always)]
fn store64be(dst: &mut [u8], v: u64) {
    dst[..8].copy_from_slice(&v.to_be_bytes());
}

// ── ASCON-128 AEAD Encrypt ────────────────────────────────────────────────────

/// ASCON-128 authenticated encryption.
///
/// # Parameters
/// - `key`        : 128-bit key (16 bytes)
/// - `nonce`      : 128-bit nonce (16 bytes) — MUST be unique per (key, message) pair
/// - `assoc_data` : Associated data (authenticated but not encrypted; may be empty)
/// - `plaintext`  : Plaintext input
/// - `ciphertext` : Ciphertext output — must be exactly `plaintext.len()` bytes
/// - `tag`        : 128-bit authentication tag output (16 bytes)
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
    assert_eq!(ciphertext.len(), plaintext.len(), "ciphertext buffer must equal plaintext length");

    // § Initialization
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

    // § Associated Data
    if !assoc_data.is_empty() {
        let mut chunks = assoc_data.chunks_exact(8);
        for chunk in chunks.by_ref() {
            s.x0 ^= load64be(chunk);
            ascon_p(&mut s, 6);
        }
        // Pad last AD block
        let rem = chunks.remainder();
        let mut pad = [0u8; 8];
        pad[..rem.len()].copy_from_slice(rem);
        pad[rem.len()] = 0x80;
        s.x0 ^= u64::from_be_bytes(pad);
        ascon_p(&mut s, 6);
    }
    s.x4 ^= 1; // domain separation

    // § Plaintext Encryption
    let mut pt_chunks = plaintext.chunks_exact(8);
    let mut ct_chunks = ciphertext.chunks_exact_mut(8);
    for (pt, ct) in pt_chunks.by_ref().zip(ct_chunks.by_ref()) {
        s.x0 ^= load64be(pt);
        store64be(ct, s.x0);
        ascon_p(&mut s, 6);
    }
    // Final partial block
    let pt_rem = pt_chunks.remainder();
    let ct_rem = ct_chunks.into_remainder();
    let mut pad = [0u8; 8];
    pad[..pt_rem.len()].copy_from_slice(pt_rem);
    pad[pt_rem.len()] = 0x80;
    let padded = u64::from_be_bytes(pad);
    s.x0 ^= padded;
    let out = s.x0.to_be_bytes();
    ct_rem.copy_from_slice(&out[..ct_rem.len()]);

    // § Finalization
    s.x1 ^= load64be(&key[0..8]);
    s.x2 ^= load64be(&key[8..16]);
    ascon_p(&mut s, 12);
    s.x3 ^= load64be(&key[0..8]);
    s.x4 ^= load64be(&key[8..16]);
    store64be(&mut tag[0..8], s.x3);
    store64be(&mut tag[8..16], s.x4);
}

// ── ASCON-128 AEAD Decrypt ────────────────────────────────────────────────────

/// ASCON-128 authenticated decryption.
///
/// Returns `true` if the tag is valid, `false` otherwise.
/// The `plaintext` buffer is zeroed on authentication failure.
pub fn ascon_aead_decrypt(
    key: &[u8; 16],
    nonce: &[u8; 16],
    assoc_data: &[u8],
    ciphertext: &[u8],
    plaintext: &mut [u8],
    tag: &[u8; 16],
) -> bool {
    assert_eq!(plaintext.len(), ciphertext.len(), "plaintext buffer must equal ciphertext length");

    // § Initialization
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

    // § Associated Data
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

    // § Ciphertext Decryption
    let mut ct_chunks = ciphertext.chunks_exact(8);
    let mut pt_chunks = plaintext.chunks_exact_mut(8);
    for (ct, pt) in ct_chunks.by_ref().zip(pt_chunks.by_ref()) {
        let c = load64be(ct);
        let p = s.x0 ^ c;
        store64be(pt, p);
        s.x0 = c; // absorb ciphertext
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

    // § Finalization
    s.x1 ^= load64be(&key[0..8]);
    s.x2 ^= load64be(&key[8..16]);
    ascon_p(&mut s, 12);
    s.x3 ^= load64be(&key[0..8]);
    s.x4 ^= load64be(&key[8..16]);

    let mut expected = [0u8; 16];
    store64be(&mut expected[0..8], s.x3);
    store64be(&mut expected[8..16], s.x4);

    // Constant-time tag comparison
    let ok = bool::from(tag.ct_eq(&expected));
    if !ok {
        // Zeroize plaintext on failure
        plaintext.zeroize();
    }
    ok
}

// ── ASCON-HASH ────────────────────────────────────────────────────────────────

/// ASCON-HASH: produces a 256-bit digest.
/// Rate = 64 bits (8 bytes), absorbs with p^12, squeezes with p^12.
pub fn ascon_hash(data: &[u8], out: &mut [u8; 32]) {
    let mut s = State {
        x0: ASCON_HASH_IV[0],
        x1: ASCON_HASH_IV[1],
        x2: ASCON_HASH_IV[2],
        x3: ASCON_HASH_IV[3],
        x4: ASCON_HASH_IV[4],
    };

    // Absorb
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

    // Squeeze 256 bits = 4 × 64-bit blocks
    store64be(&mut out[0..8],   s.x0); ascon_p(&mut s, 12);
    store64be(&mut out[8..16],  s.x0); ascon_p(&mut s, 12);
    store64be(&mut out[16..24], s.x0); ascon_p(&mut s, 12);
    store64be(&mut out[24..32], s.x0);
}

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AsconError {
    AuthenticationFailed,
    BufferTooSmall,
}

#[cfg(test)]
mod tests;
