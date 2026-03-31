use subtle::ConstantTimeEq;
use zeroize::Zeroize;

#[derive(Clone, Zeroize)]
#[zeroize(drop)]
pub struct State {
    pub x0: u64,
    pub x1: u64,
    pub x2: u64,
    pub x3: u64,
    pub x4: u64,
}

const ROUND_CONSTANTS: [u64; 12] = [
    0x00000000000000f0, 0x00000000000000e1, 0x00000000000000d2, 0x00000000000000c3,
    0x00000000000000b4, 0x00000000000000a5, 0x0000000000000096, 0x0000000000000087,
    0x0000000000000078, 0x0000000000000069, 0x000000000000005a, 0x000000000000004b,
];

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

#[inline(always)]
pub fn ascon_round(s: &mut State, rc: u64) {
    s.x2 ^= rc;
    ascon_sbox(s);
    s.x0 ^= s.x0.rotate_right(19) ^ s.x0.rotate_right(28);
    s.x1 ^= s.x1.rotate_right(61) ^ s.x1.rotate_right(39);
    s.x2 ^= s.x2.rotate_right(1) ^ s.x2.rotate_right(6);
    s.x3 ^= s.x3.rotate_right(10) ^ s.x3.rotate_right(17);
    s.x4 ^= s.x4.rotate_right(7) ^ s.x4.rotate_right(41);
}

pub fn ascon_p(s: &mut State, rounds: usize) {
    let start = 12 - rounds;
    for i in start..12 {
        ascon_round(s, ROUND_CONSTANTS[i]);
    }
}

pub fn ascon_aead_encrypt(key: &[u8; 16], nonce: &[u8; 16], assoc_data: &[u8], plaintext: &[u8], ciphertext: &mut [u8], tag: &mut [u8; 16]) {
    let mut s = State {
        x0: 0x80400c0600000000,
        x1: u64::from_be_bytes(key[0..8].try_into().unwrap()),
        x2: u64::from_be_bytes(key[8..16].try_into().unwrap()),
        x3: u64::from_be_bytes(nonce[0..8].try_into().unwrap()),
        x4: u64::from_be_bytes(nonce[8..16].try_into().unwrap()),
    };
    ascon_p(&mut s, 12);
    s.x3 ^= u64::from_be_bytes(key[0..8].try_into().unwrap());
    s.x4 ^= u64::from_be_bytes(key[8..16].try_into().unwrap());

    if !assoc_data.is_empty() {
        let mut ad_chunks = assoc_data.chunks_exact(8);
        for chunk in &mut ad_chunks {
            s.x0 ^= u64::from_be_bytes(chunk.try_into().unwrap());
            ascon_p(&mut s, 6);
        }
        let remainder = ad_chunks.remainder();
        let mut padded = [0u8; 8];
        padded[..remainder.len()].copy_from_slice(remainder);
        padded[remainder.len()] = 0x80;
        s.x0 ^= u64::from_be_bytes(padded);
        ascon_p(&mut s, 6);
    }
    s.x4 ^= 1;

    let mut pt_chunks = plaintext.chunks_exact(8);
    let mut ct_chunks = ciphertext.chunks_exact_mut(8);
    for (pt_chunk, ct_chunk) in pt_chunks.by_ref().zip(ct_chunks.by_ref()) {
        s.x0 ^= u64::from_be_bytes(pt_chunk.try_into().unwrap());
        ct_chunk.copy_from_slice(&s.x0.to_be_bytes());
        ascon_p(&mut s, 6);
    }

    let pt_remainder = pt_chunks.remainder();
    let ct_remainder = ct_chunks.into_remainder();
    for i in 0..pt_remainder.len() {
        s.x0 ^= (pt_remainder[i] as u64) << (56 - 8 * i);
        ct_remainder[i] = (s.x0 >> (56 - 8 * i)) as u8;
    }
    s.x0 ^= 0x80u64 << (56 - 8 * pt_remainder.len());

    s.x1 ^= u64::from_be_bytes(key[0..8].try_into().unwrap());
    s.x2 ^= u64::from_be_bytes(key[8..16].try_into().unwrap());
    ascon_p(&mut s, 12);
    s.x3 ^= u64::from_be_bytes(key[0..8].try_into().unwrap());
    s.x4 ^= u64::from_be_bytes(key[8..16].try_into().unwrap());

    tag[0..8].copy_from_slice(&s.x3.to_be_bytes());
    tag[8..16].copy_from_slice(&s.x4.to_be_bytes());
}

pub fn ascon_aead_decrypt(key: &[u8; 16], nonce: &[u8; 16], assoc_data: &[u8], ciphertext: &[u8], plaintext: &mut [u8], tag: &[u8; 16]) -> bool {
    let mut s = State {
        x0: 0x80400c0600000000,
        x1: u64::from_be_bytes(key[0..8].try_into().unwrap()),
        x2: u64::from_be_bytes(key[8..16].try_into().unwrap()),
        x3: u64::from_be_bytes(nonce[0..8].try_into().unwrap()),
        x4: u64::from_be_bytes(nonce[8..16].try_into().unwrap()),
    };
    ascon_p(&mut s, 12);
    s.x3 ^= u64::from_be_bytes(key[0..8].try_into().unwrap());
    s.x4 ^= u64::from_be_bytes(key[8..16].try_into().unwrap());

    if !assoc_data.is_empty() {
        let mut ad_chunks = assoc_data.chunks_exact(8);
        for chunk in &mut ad_chunks {
            s.x0 ^= u64::from_be_bytes(chunk.try_into().unwrap());
            ascon_p(&mut s, 6);
        }
        let remainder = ad_chunks.remainder();
        let mut padded = [0u8; 8];
        padded[..remainder.len()].copy_from_slice(remainder);
        padded[remainder.len()] = 0x80;
        s.x0 ^= u64::from_be_bytes(padded);
        ascon_p(&mut s, 6);
    }
    s.x4 ^= 1;

    let mut ct_chunks = ciphertext.chunks_exact(8);
    let mut pt_chunks = plaintext.chunks_exact_mut(8);
    for (ct_chunk, pt_chunk) in ct_chunks.by_ref().zip(pt_chunks.by_ref()) {
        let ct_val = u64::from_be_bytes(ct_chunk.try_into().unwrap());
        let pt_val = s.x0 ^ ct_val;
        pt_chunk.copy_from_slice(&pt_val.to_be_bytes());
        s.x0 = ct_val;
        ascon_p(&mut s, 6);
    }

    let ct_remainder = ct_chunks.remainder();
    let pt_remainder = pt_chunks.into_remainder();
    for i in 0..ct_remainder.len() {
        pt_remainder[i] = ct_remainder[i] ^ (s.x0 >> (56 - 8 * i)) as u8;
        s.x0 = (s.x0 & !(0xFF << (56 - 8 * i))) | ((ct_remainder[i] as u64) << (56 - 8 * i));
    }
    s.x0 ^= 0x80u64 << (56 - 8 * pt_remainder.len());

    s.x1 ^= u64::from_be_bytes(key[0..8].try_into().unwrap());
    s.x2 ^= u64::from_be_bytes(key[8..16].try_into().unwrap());
    ascon_p(&mut s, 12);
    s.x3 ^= u64::from_be_bytes(key[0..8].try_into().unwrap());
    s.x4 ^= u64::from_be_bytes(key[8..16].try_into().unwrap());

    let mut expected_tag = [0u8; 16];
    expected_tag[0..8].copy_from_slice(&s.x3.to_be_bytes());
    expected_tag[8..16].copy_from_slice(&s.x4.to_be_bytes());

    bool::from(tag.ct_eq(&expected_tag))
}

pub fn ascon_hash(data: &[u8], out: &mut [u8; 32]) {
    // ASCON-HASH initial state
    let mut s = State {
        x0: 0xee9398aadb67f03d,
        x1: 0x8bb21831c60f1002,
        x2: 0xb48a92db98d5da62,
        x3: 0x43189921b8f8e3e8,
        x4: 0x348fa5c9d525e140,
    };

    // Absorb
    let mut chunks = data.chunks_exact(8);
    for chunk in &mut chunks {
        s.x0 ^= u64::from_be_bytes(chunk.try_into().unwrap());
        ascon_p(&mut s, 12);
    }

    let remainder = chunks.remainder();
    let mut padded = [0u8; 8];
    padded[..remainder.len()].copy_from_slice(remainder);
    padded[remainder.len()] = 0x80;
    
    s.x0 ^= u64::from_be_bytes(padded);
    ascon_p(&mut s, 12);

    // Squeeze 256 bits (32 bytes) = 4 blocks of 8 bytes
    out[0..8].copy_from_slice(&s.x0.to_be_bytes());
    ascon_p(&mut s, 12);
    out[8..16].copy_from_slice(&s.x0.to_be_bytes());
    ascon_p(&mut s, 12);
    out[16..24].copy_from_slice(&s.x0.to_be_bytes());
    ascon_p(&mut s, 12);
    out[24..32].copy_from_slice(&s.x0.to_be_bytes());
}
