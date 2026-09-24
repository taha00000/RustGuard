//! A registry of cryptographic primitives to evaluate for timing leakage.
//!
//! Each `Probe` exposes one *verification* operation â€” the path where a secret
//! tag/MAC is compared â€” behind a uniform interface, so the same firmware
//! harness, capture driver, and analysis can be pointed at any of them. This is
//! what turns a single-implementation study into a systematic evaluation of the
//! Rust cryptographic ecosystem on embedded targets.
//!
//! ## Why verification paths
//! On a cacheless Cortex-M4 the classic cache-timing leak classes (AES T-tables,
//! GHASH tables) do not manifest â€” a table lookup costs the same regardless of
//! index. The leak classes that *do* manifest are:
//!   1. secret-dependent branches (early returns),
//!   2. variable-latency arithmetic (`UDIV`/`SDIV` are 2-12 cycles on M4),
//!   3. early-return comparisons â€” canonically, tag/MAC verification.
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
    /// Public-key: the secret is a scalar, and the timed operation is the
    /// crate's own scalar multiplication.
    Pk,
}

/// Largest key any probe takes (ChaCha20Poly1305 = 32).
pub const MAX_KEY: usize = 32;

pub struct Probe {
    pub id: u8,
    pub name: &'static str,
    pub kind: Kind,
    pub tag_len: usize,
    pub key_len: usize,
    /// Run the crate's own verification with `tag`. This is the timed operation
    /// of the `verify` experiment.
    pub verify: fn(tag: &[u8]) -> bool,
    /// Write the genuine tag for the fixed key/message; returns its length.
    pub correct_tag: fn(out: &mut [u8; MAX_TAG]) -> usize,
    /// Authenticate the fixed message under `key`. The timed operation of the
    /// `keyed` experiment: it exercises the primitive's core (key schedule,
    /// block function, field arithmetic) under a secret that actually varies,
    /// which the verify experiment â€” fixed key, varying tag â€” never touches.
    /// Returns a tag byte so the work cannot be optimized away.
    pub encrypt_keyed: fn(key: &[u8]) -> u8,
}

const MSG: [u8; 16] = [0x11; 16];

// â”€â”€ AEAD probes â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
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

            pub fn encrypt_keyed(key: &[u8]) -> u8 {
                let c = <C as AeadKeyInit>::new_from_slice(&key[..$klen]).unwrap();
                let mut buf = MSG;
                let t = c
                    .encrypt_in_place_detached(aead::Nonce::<C>::from_slice(&NONCE), &[], &mut buf)
                    .unwrap();
                core::hint::black_box(t[0])
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

// â”€â”€ MAC probes â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
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

            pub fn encrypt_keyed(key: &[u8]) -> u8 {
                let mut m = <M as Mac>::new_from_slice(&key[..$klen]).unwrap();
                m.update(&MSG);
                core::hint::black_box(m.finalize().into_bytes()[0])
            }
        }
    };
}

mac_probe!(p_hmac_sha256, hmac::Hmac<sha2::Sha256>, 32);
mac_probe!(p_cmac_aes, cmac::Cmac<aes::Aes128>, 16);

// â”€â”€ RustGuard's own ASCON: the validated control pair â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
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
    pub fn encrypt_keyed(key: &[u8]) -> u8 {
        let mut k = [0u8; 16];
        k.copy_from_slice(&key[..16]);
        let mut ct = [0u8; 16];
        let mut tag = [0u8; 16];
        ascon_aead_encrypt(&k, &NONCE, &[], &MSG, &mut ct, &mut tag);
        core::hint::black_box(tag[0])
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
    pub fn encrypt_keyed(key: &[u8]) -> u8 {
        p_rustguard::encrypt_keyed(key)
    }
}

#[cfg(feature = "leaky-control")]
mod p_canary {
    //! Optimizer-proof positive control.
    //!
    //! The `p_rustguard_leaky` control is a *realistic* leak (an early-exit tag
    //! comparison), and at -O2/-O3 LLVM rewrites it into branchless code, so it
    //! stops leaking â€” a finding in its own right, but it leaves those columns
    //! with no positive control. This control instead spends a number of cycles
    //! taken directly from the tag, behind `black_box` so the compiler may not
    //! reason about or remove it. It leaks by construction at every
    //! optimization level, which is what makes a "no leakage detected" verdict
    //! in the same column mean something.
    use super::*;

    pub fn correct(out: &mut [u8; MAX_TAG]) -> usize {
        p_rustguard::correct(out)
    }
    pub fn verify(tag: &[u8]) -> bool {
        let n = core::hint::black_box(tag[0]) as usize & 0x3F;
        let mut acc = 0u32;
        for i in 0..n {
            acc = core::hint::black_box(acc.wrapping_add(i as u32));
        }
        core::hint::black_box(acc);
        false // always rejects: no secret is revealed, only cycles are spent
    }
    /// Same construction on the key side, so the `keyed` experiment also has a
    /// control that leaks by construction at every optimization level.
    pub fn encrypt_keyed(key: &[u8]) -> u8 {
        let n = core::hint::black_box(key[0]) as usize & 0x3F;
        let mut acc = 0u32;
        for i in 0..n {
            acc = core::hint::black_box(acc.wrapping_add(i as u32));
        }
        core::hint::black_box(acc as u8)
    }
}

// ── calibrated leak ladder ───────────────────────────────────────────────────
// Controls that answer "how small a leak can this platform see?". Each spends a
// known number of extra loop iterations when the secret's first bit is set, so
// the leak's magnitude is known by construction and grows by powers of two. The
// smallest rung a platform still flags is that platform's detection floor —
// one cycle on a microcontroller with a deterministic counter, far coarser on an
// OS-scheduled application core. Without this, "no leakage detected" has no
// scale attached to it.
#[cfg(feature = "ladder")]
macro_rules! ladder_probe {
    ($m:ident, $iters:expr) => {
        mod $m {
            use super::*;
            fn spend(secret_byte: u8) -> u8 {
                let extra = if core::hint::black_box(secret_byte) & 1 == 1 {
                    $iters
                } else {
                    0
                };
                let mut acc = 0u32;
                for i in 0..extra {
                    acc = core::hint::black_box(acc.wrapping_add(i as u32));
                }
                core::hint::black_box(acc as u8)
            }
            pub fn correct(out: &mut [u8; MAX_TAG]) -> usize {
                p_rustguard::correct(out)
            }
            pub fn verify(tag: &[u8]) -> bool {
                let _ = spend(tag[0]);
                false
            }
            pub fn encrypt_keyed(key: &[u8]) -> u8 {
                spend(key[0])
            }
        }
    };
}

#[cfg(feature = "ladder")]
ladder_probe!(p_ladder1, 1u32);
#[cfg(feature = "ladder")]
ladder_probe!(p_ladder2, 2u32);
#[cfg(feature = "ladder")]
ladder_probe!(p_ladder4, 4u32);
#[cfg(feature = "ladder")]
ladder_probe!(p_ladder8, 8u32);
#[cfg(feature = "ladder")]
ladder_probe!(p_ladder16, 16u32);
#[cfg(feature = "ladder")]
ladder_probe!(p_ladder64, 64u32);
#[cfg(feature = "ladder")]
ladder_probe!(p_ladder256, 256u32);

// ── public-key probes ────────────────────────────────────────────────────────
// Block ciphers and MACs are the easy case: they are branch-free by
// construction, which is why they measure clean. Scalar multiplication is where
// timing bugs actually occur, so the secret here is the scalar itself and the
// timed operation is the crate's own variable-base/fixed-base multiplication.
#[cfg(feature = "pubkey")]
mod p_x25519 {
    use super::*;
    use curve25519_dalek::montgomery::MontgomeryPoint;

    pub fn correct(out: &mut [u8; MAX_TAG]) -> usize {
        let mut s = [0u8; 32];
        s.copy_from_slice(&[0x42u8; 32]);
        out[..32].copy_from_slice(&MontgomeryPoint::mul_base_clamped(s).to_bytes());
        32
    }
    pub fn verify(tag: &[u8]) -> bool {
        // x25519 has no verification API of its own, so this derives the public
        // key and compares. The comparison must use `subtle`: a plain `==` on
        // slices is an early-exit memcmp, which would leak and be measured as
        // though the crate leaked, when the artefact is this probe's own code.
        use subtle::ConstantTimeEq;
        let mut s = [0u8; 32];
        s.copy_from_slice(&[0x42u8; 32]);
        let pk = MontgomeryPoint::mul_base_clamped(s).to_bytes();
        bool::from(pk[..].ct_eq(&tag[..32.min(tag.len())]))
    }
    pub fn encrypt_keyed(key: &[u8]) -> u8 {
        let mut s = [0u8; 32];
        s.copy_from_slice(&key[..32]);
        core::hint::black_box(MontgomeryPoint::mul_base_clamped(s).to_bytes()[0])
    }
}

#[cfg(feature = "pubkey")]
mod p_p256 {
    use super::*;
    use p256::elliptic_curve::ops::Reduce;
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    use p256::{ProjectivePoint, Scalar, U256};

    fn scalar_from(bytes: &[u8]) -> Scalar {
        let mut b = [0u8; 32];
        b.copy_from_slice(&bytes[..32]);
        // Reduce rather than reject: a uniformly random 32-byte string is not
        // always a valid scalar, and rejection would make the timing depend on
        // the retry count rather than on the crate.
        Scalar::reduce(U256::from_be_slice(&b))
    }

    pub fn correct(out: &mut [u8; MAX_TAG]) -> usize {
        let p = ProjectivePoint::GENERATOR * scalar_from(&[0x42u8; 32]);
        let enc = p.to_affine().to_encoded_point(true);
        let b = enc.as_bytes();
        let n = core::cmp::min(b.len(), MAX_TAG);
        out[..n].copy_from_slice(&b[..n]);
        n
    }
    pub fn verify(tag: &[u8]) -> bool {
        use subtle::ConstantTimeEq;
        let mut buf = [0u8; MAX_TAG];
        let n = correct(&mut buf);
        bool::from(buf[..n].ct_eq(&tag[..n.min(tag.len())]))
    }
    pub fn encrypt_keyed(key: &[u8]) -> u8 {
        let p = ProjectivePoint::GENERATOR * scalar_from(key);
        core::hint::black_box(p.to_affine().to_encoded_point(true).as_bytes()[1])
    }
}

macro_rules! entry {
    ($id:expr, $name:expr, $kind:expr, $len:expr, $klen:expr, $m:ident) => {
        Probe {
            id: $id,
            name: $name,
            kind: $kind,
            tag_len: $len,
            key_len: $klen,
            verify: $m::verify,
            correct_tag: $m::correct,
            encrypt_keyed: $m::encrypt_keyed,
        }
    };
}

/// Every probe available in this build. Ids are stable across builds so results
/// can be joined across boards and optimization levels.
pub static PROBES: &[Probe] = &[
    entry!(0, "rustguard-ascon128", Kind::Aead, 16, 16, p_rustguard),
    entry!(1, "ascon-aead", Kind::Aead, 16, 16, p_ascon_rc),
    entry!(2, "chacha20poly1305", Kind::Aead, 16, 32, p_chachapoly),
    entry!(3, "aes-gcm", Kind::Aead, 16, 16, p_aesgcm),
    entry!(4, "aes-gcm-siv", Kind::Aead, 16, 16, p_aesgcmsiv),
    entry!(5, "eax-aes128", Kind::Aead, 16, 16, p_aeseax),
    entry!(6, "ccm-aes128", Kind::Aead, 16, 16, p_aesccm),
    entry!(7, "hmac-sha256", Kind::Mac, 32, 32, p_hmac_sha256),
    entry!(8, "cmac-aes128", Kind::Mac, 16, 16, p_cmac_aes),
    #[cfg(feature = "pubkey")]
    entry!(20, "x25519-dalek", Kind::Pk, 32, 32, p_x25519),
    #[cfg(feature = "pubkey")]
    entry!(21, "p256-scalarmul", Kind::Pk, 32, 32, p_p256),
    #[cfg(feature = "ladder")]
    entry!(90, "LADDER-1", Kind::Aead, 16, 16, p_ladder1),
    #[cfg(feature = "ladder")]
    entry!(91, "LADDER-2", Kind::Aead, 16, 16, p_ladder2),
    #[cfg(feature = "ladder")]
    entry!(92, "LADDER-4", Kind::Aead, 16, 16, p_ladder4),
    #[cfg(feature = "ladder")]
    entry!(93, "LADDER-8", Kind::Aead, 16, 16, p_ladder8),
    #[cfg(feature = "ladder")]
    entry!(94, "LADDER-16", Kind::Aead, 16, 16, p_ladder16),
    #[cfg(feature = "ladder")]
    entry!(95, "LADDER-64", Kind::Aead, 16, 16, p_ladder64),
    #[cfg(feature = "ladder")]
    entry!(96, "LADDER-256", Kind::Aead, 16, 16, p_ladder256),
    #[cfg(feature = "leaky-control")]
    entry!(98, "CANARY-control", Kind::Aead, 16, 16, p_canary),
    #[cfg(feature = "leaky-control")]
    entry!(99, "rustguard-LEAKY-control", Kind::Aead, 16, 16, p_rustguard_leaky),
];

pub fn find(id: u8) -> Option<&'static Probe> {
    PROBES.iter().find(|p| p.id == id)
}



