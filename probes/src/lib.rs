//! A registry of cryptographic primitives to evaluate for timing leakage.
//!
//! Each `Probe` exposes one *verification* operation — the path where a secret
//! tag/MAC is compared — behind a uniform interface, so the same firmware
//! harness, capture driver, and analysis can be pointed at any of them. This is
//! what turns a single-implementation study into a systematic evaluation of the
//! Rust cryptographic ecosystem on embedded targets.
//!
//! ## Why verification paths
//! On a cacheless Cortex-M4 the classic cache-timing leak classes (AES T-tables,
//! GHASH tables) do not manifest — a table lookup costs the same regardless of
//! index. The leak classes that *do* manifest are:
//!   1. secret-dependent branches (early returns),
//!   2. variable-latency arithmetic (`UDIV`/`SDIV` are 2-12 cycles on M4),
//!   3. early-return comparisons — canonically, tag/MAC verification.
//! So verification is where the yield is, and every probe here measures it.
//!
//! ## Experiment design (matches capture/collect_timing.py)
//! Both classes present a *wrong* tag, so both reject and both run the same
//! failure path; only the compare differs:
//!   * fixed class  : the correct tag with its last byte flipped (long prefix match)
//!   * random class : a uniformly random tag (mismatches almost immediately)
//! A constant-time compare is identical for both (|t| ~ 0); an early-return
//! compare is not (|t| >> 4.5).

#![no_std]

use aead::{AeadInPlace, KeyInit as AeadKeyInit};
use digest::Mac;

/// Largest tag any probe produces (HMAC-SHA256 = 32).
pub const MAX_TAG: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Aead,
    Mac,
}

pub struct Probe {
    pub id: u8,
    pub name: &'static str,
    pub kind: Kind,
    pub tag_len: usize,
    /// Run the crate's own verification with `tag`. This is the timed operation.
    pub verify: fn(tag: &[u8]) -> bool,
    /// Write the genuine tag for the fixed key/message; returns its length.
    pub correct_tag: fn(out: &mut [u8; MAX_TAG]) -> usize,
}

const MSG: [u8; 16] = [0x11; 16];

// ── AEAD probes ──────────────────────────────────────────────────────────────
// Each measures `decrypt_in_place_detached`, which recomputes the tag and runs
// the crate's own comparison. The ciphertext is rebuilt identically for both
// classes, so that constant cost cancels in the t-test.

macro_rules! aead_probe {
    ($m:ident, $ty:ty, $klen:expr, $nlen:expr) => {
        mod $m {
            use super::*;
            type C = $ty;
            const KEY: [u8; $klen] = [0x42; $klen];
            const NONCE: [u8; $nlen] = [0xAA; $nlen];

            fn cipher() -> C {
                <C as AeadKeyInit>::new_from_slice(&KEY).unwrap()
            }

            pub fn correct(out: &mut [u8; MAX_TAG]) -> usize {
                let c = cipher();
                let mut buf = MSG;
                let t = c
                    .encrypt_in_place_detached(aead::Nonce::<C>::from_slice(&NONCE), &[], &mut buf)
                    .unwrap();
                out[..t.len()].copy_from_slice(&t);
                t.len()
            }

            pub fn verify(tag: &[u8]) -> bool {
                let c = cipher();
                // Rebuild the genuine ciphertext so only the tag differs.
                let mut buf = MSG;
                let _ = c.encrypt_in_place_detached(
                    aead::Nonce::<C>::from_slice(&NONCE),
                    &[],
                    &mut buf,
                );
                c.decrypt_in_place_detached(
                    aead::Nonce::<C>::from_slice(&NONCE),
                    &[],
                    &mut buf,
                    aead::Tag::<C>::from_slice(tag),
                )
                .is_ok()
            }
        }
    };
}

aead_probe!(p_ascon_rc, ascon_aead::Ascon128, 16, 16);
aead_probe!(p_chachapoly, chacha20poly1305::ChaCha20Poly1305, 32, 12);
aead_probe!(p_aesgcm, aes_gcm::Aes128Gcm, 16, 12);
aead_probe!(p_aesgcmsiv, aes_gcm_siv::Aes128GcmSiv, 16, 12);
aead_probe!(p_aeseax, eax::Eax<aes::Aes128>, 16, 16);

// AES-CCM needs explicit tag/nonce sizes.
type Aes128Ccm = ccm::Ccm<aes::Aes128, ccm::consts::U16, ccm::consts::U13>;
aead_probe!(p_aesccm, Aes128Ccm, 16, 13);

// ── MAC probes ───────────────────────────────────────────────────────────────
// Each measures the crate's own `verify_slice`, the canonical early-return risk.

macro_rules! mac_probe {
    ($m:ident, $ty:ty, $klen:expr) => {
        mod $m {
            use super::*;
            type M = $ty;
            const KEY: [u8; $klen] = [0x42; $klen];

            pub fn correct(out: &mut [u8; MAX_TAG]) -> usize {
                let mut m = <M as Mac>::new_from_slice(&KEY).unwrap();
                m.update(&MSG);
                let t = m.finalize().into_bytes();
                out[..t.len()].copy_from_slice(&t);
                t.len()
            }

            pub fn verify(tag: &[u8]) -> bool {
                let mut m = <M as Mac>::new_from_slice(&KEY).unwrap();
                m.update(&MSG);
                m.verify_slice(tag).is_ok()
            }
        }
    };
}

mac_probe!(p_hmac_sha256, hmac::Hmac<sha2::Sha256>, 32);
mac_probe!(p_cmac_aes, cmac::Cmac<aes::Aes128>, 16);

// ── RustGuard's own ASCON: the validated control pair ────────────────────────
// The constant-time path and (under `leaky-control`) the deliberately
// variable-time path. These are the positive/negative controls that prove the
// method can detect a real leak on this hardware.

mod p_rustguard {
    use super::*;
    use rustguard_core::{ascon_aead_decrypt, ascon_aead_encrypt};
    const KEY: [u8; 16] = [0x42; 16];
    const NONCE: [u8; 16] = [0xAA; 16];

    pub fn correct(out: &mut [u8; MAX_TAG]) -> usize {
        let mut ct = [0u8; 16];
        let mut tag = [0u8; 16];
        ascon_aead_encrypt(&KEY, &NONCE, &[], &MSG, &mut ct, &mut tag);
        out[..16].copy_from_slice(&tag);
        16
    }
    pub fn verify(tag: &[u8]) -> bool {
        let mut ct = [0u8; 16];
        let mut real = [0u8; 16];
        ascon_aead_encrypt(&KEY, &NONCE, &[], &MSG, &mut ct, &mut real);
        let mut t = [0u8; 16];
        t.copy_from_slice(&tag[..16]);
        let mut rec = [0u8; 16];
        ascon_aead_decrypt(&KEY, &NONCE, &[], &ct, &mut rec, &t)
    }
}

#[cfg(feature = "leaky-control")]
mod p_rustguard_leaky {
    use super::*;
    use rustguard_core::{ascon_aead_decrypt_variabletime, ascon_aead_encrypt};
    const KEY: [u8; 16] = [0x42; 16];
    const NONCE: [u8; 16] = [0xAA; 16];

    pub fn correct(out: &mut [u8; MAX_TAG]) -> usize {
        let mut ct = [0u8; 16];
        let mut tag = [0u8; 16];
        ascon_aead_encrypt(&KEY, &NONCE, &[], &MSG, &mut ct, &mut tag);
        out[..16].copy_from_slice(&tag);
        16
    }
    pub fn verify(tag: &[u8]) -> bool {
        let mut ct = [0u8; 16];
        let mut real = [0u8; 16];
        ascon_aead_encrypt(&KEY, &NONCE, &[], &MSG, &mut ct, &mut real);
        let mut t = [0u8; 16];
        t.copy_from_slice(&tag[..16]);
        let mut rec = [0u8; 16];
        ascon_aead_decrypt_variabletime(&KEY, &NONCE, &[], &ct, &mut rec, &t)
    }
}

macro_rules! entry {
    ($id:expr, $name:expr, $kind:expr, $len:expr, $m:ident) => {
        Probe {
            id: $id,
            name: $name,
            kind: $kind,
            tag_len: $len,
            verify: $m::verify,
            correct_tag: $m::correct,
        }
    };
}

/// Every probe available in this build. Ids are stable across builds so results
/// can be joined across boards and optimization levels.
pub static PROBES: &[Probe] = &[
    entry!(0, "rustguard-ascon128", Kind::Aead, 16, p_rustguard),
    entry!(1, "ascon-aead", Kind::Aead, 16, p_ascon_rc),
    entry!(2, "chacha20poly1305", Kind::Aead, 16, p_chachapoly),
    entry!(3, "aes-gcm", Kind::Aead, 16, p_aesgcm),
    entry!(4, "aes-gcm-siv", Kind::Aead, 16, p_aesgcmsiv),
    entry!(5, "eax-aes128", Kind::Aead, 16, p_aeseax),
    entry!(6, "ccm-aes128", Kind::Aead, 16, p_aesccm),
    entry!(7, "hmac-sha256", Kind::Mac, 32, p_hmac_sha256),
    entry!(8, "cmac-aes128", Kind::Mac, 16, p_cmac_aes),
    #[cfg(feature = "leaky-control")]
    entry!(99, "rustguard-LEAKY-control", Kind::Aead, 16, p_rustguard_leaky),
];

pub fn find(id: u8) -> Option<&'static Probe> {
    PROBES.iter().find(|p| p.id == id)
}
