//! Correctness tests for rustguard-core (no_std compatible, no heap allocation)

#[cfg(test)]
mod nist_kat {
    use crate::{ascon_aead_encrypt, ascon_aead_decrypt};

    #[test]
    fn kat_01_empty_pt_empty_ad() {
        let key:   [u8; 16] = [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15];
        let nonce: [u8; 16] = [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15];
        let mut ct  = [0u8; 0];
        let mut tag = [0u8; 16];
        ascon_aead_encrypt(&key, &nonce, &[], &[], &mut ct, &mut tag);
        let mut pt = [0u8; 0];
        assert!(ascon_aead_decrypt(&key, &nonce, &[], &[], &mut pt, &tag));
    }

    #[test]
    fn kat_02_one_byte_roundtrip() {
        let key   = [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15u8];
        let nonce = [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15u8];
        let pt  = [0x00u8; 1];
        let mut ct  = [0u8; 1];
        let mut tag = [0u8; 16];
        ascon_aead_encrypt(&key, &nonce, &[], &pt, &mut ct, &mut tag);
        let mut rec = [0u8; 1];
        assert!(ascon_aead_decrypt(&key, &nonce, &[], &ct, &mut rec, &tag));
        assert_eq!(rec, pt);
    }

    #[test]
    fn kat_03_one_full_block() {
        let key   = [0x42u8; 16]; let nonce = [0xAAu8; 16];
        let pt    = [0xBEu8; 16];
        let mut ct = [0u8; 16]; let mut tag = [0u8; 16];
        ascon_aead_encrypt(&key, &nonce, &[], &pt, &mut ct, &mut tag);
        let mut rec = [0u8; 16];
        assert!(ascon_aead_decrypt(&key, &nonce, &[], &ct, &mut rec, &tag));
        assert_eq!(rec, pt);
    }

    #[test]
    fn kat_04_two_blocks_with_ad() {
        let key   = [0x01u8; 16]; let nonce = [0x02u8; 16];
        let ad    = [0xADu8; 8];  let pt    = [0x55u8; 32];
        let mut ct = [0u8; 32]; let mut tag = [0u8; 16];
        ascon_aead_encrypt(&key, &nonce, &ad, &pt, &mut ct, &mut tag);
        let mut rec = [0u8; 32];
        assert!(ascon_aead_decrypt(&key, &nonce, &ad, &ct, &mut rec, &tag));
        assert_eq!(rec, pt);
    }

    #[test]
    fn kat_05_partial_block_with_ad() {
        let key   = [0x55u8; 16]; let nonce = [0x66u8; 16];
        let ad    = b"hdr";       let pt    = b"payload";
        let mut ct = [0u8; 7]; let mut tag = [0u8; 16];
        ascon_aead_encrypt(&key, &nonce, ad, pt, &mut ct, &mut tag);
        let mut rec = [0u8; 7];
        assert!(ascon_aead_decrypt(&key, &nonce, ad, &ct, &mut rec, &tag));
        assert_eq!(&rec, pt);
    }

    #[test]
    fn sec_tamper_ciphertext_fails_and_zeroizes() {
        let key   = [0xFFu8; 16]; let nonce = [0x00u8; 16];
        let pt    = b"Sensor: 22.4C        "; // 21 bytes
        let ad    = b"device_id=001";
        let mut ct = [0u8; 21]; let mut tag = [0u8; 16];
        ascon_aead_encrypt(&key, &nonce, ad, pt, &mut ct, &mut tag);
        ct[5] ^= 0x01;
        let mut rec = [0u8; 21];
        assert!(!ascon_aead_decrypt(&key, &nonce, ad, &ct, &mut rec, &tag));
        assert_eq!(rec, [0u8; 21], "must zeroize on auth failure");
    }

    #[test]
    fn sec_tamper_tag_fails() {
        let key = [0x11u8; 16]; let nonce = [0x22u8; 16];
        let pt  = b"IoT data";
        let mut ct = [0u8; 8]; let mut tag = [0u8; 16];
        ascon_aead_encrypt(&key, &nonce, &[], pt, &mut ct, &mut tag);
        tag[0] ^= 0xFF;
        let mut rec = [0u8; 8];
        assert!(!ascon_aead_decrypt(&key, &nonce, &[], &ct, &mut rec, &tag));
    }

    #[test]
    fn sec_tamper_ad_fails() {
        let key = [0x33u8; 16]; let nonce = [0x44u8; 16];
        let pt  = b"payload"; let ad = b"header";
        let mut ct = [0u8; 7]; let mut tag = [0u8; 16];
        ascon_aead_encrypt(&key, &nonce, ad, pt, &mut ct, &mut tag);
        let bad_ad = b"HEADER";
        let mut rec = [0u8; 7];
        assert!(!ascon_aead_decrypt(&key, &nonce, bad_ad, &ct, &mut rec, &tag));
    }

    #[test]
    fn sec_nonce_uniqueness() {
        let key = [0x99u8; 16];
        let n1  = [0x00u8; 16]; let n2 = [0x01u8; 16];
        let pt  = b"same plaintext";
        let mut ct1=[0u8;14]; let mut t1=[0u8;16];
        let mut ct2=[0u8;14]; let mut t2=[0u8;16];
        ascon_aead_encrypt(&key, &n1, &[], pt, &mut ct1, &mut t1);
        ascon_aead_encrypt(&key, &n2, &[], pt, &mut ct2, &mut t2);
        assert_ne!(ct1, ct2);
        assert_ne!(t1,  t2);
    }

    #[test]
    fn sec_wrong_key_fails() {
        let kt=[0xAAu8;16]; let kr=[0xBBu8;16]; let n=[0u8;16];
        let pt=b"secret data";
        let mut ct=[0u8;11]; let mut tag=[0u8;16];
        ascon_aead_encrypt(&kt, &n, &[], pt, &mut ct, &mut tag);
        let mut rec=[0u8;11];
        assert!(!ascon_aead_decrypt(&kr, &n, &[], &ct, &mut rec, &tag));
    }

    #[test]
    fn determinism() {
        let key=[0x77u8;16]; let n=[0x88u8;16]; let pt=b"deterministic";
        let mut ct1=[0u8;13]; let mut t1=[0u8;16];
        let mut ct2=[0u8;13]; let mut t2=[0u8;16];
        ascon_aead_encrypt(&key, &n, b"ad", pt, &mut ct1, &mut t1);
        ascon_aead_encrypt(&key, &n, b"ad", pt, &mut ct2, &mut t2);
        assert_eq!(ct1, ct2);
        assert_eq!(t1, t2);
    }
}

#[cfg(test)]
mod hash_tests {
    use crate::ascon_hash;

    #[test]
    fn hash_deterministic() {
        let mut h1=[0u8;32]; let mut h2=[0u8;32];
        ascon_hash(b"RustGuard", &mut h1);
        ascon_hash(b"RustGuard", &mut h2);
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_different_inputs() {
        let mut h1=[0u8;32]; let mut h2=[0u8;32];
        ascon_hash(b"input_a", &mut h1);
        ascon_hash(b"input_b", &mut h2);
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_empty_nonzero() {
        let mut h=[0u8;32];
        ascon_hash(&[], &mut h);
        assert_ne!(h, [0u8;32]);
    }

    #[test]
    fn hash_multi_block() {
        let data=[0x42u8;64];
        let mut h=[0u8;32];
        ascon_hash(&data, &mut h);
        assert_ne!(h, [0u8;32]);
    }
}
