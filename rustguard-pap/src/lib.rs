//! rustguard-pap — RustGuard Packet Authentication Protocol
//!
//! ## Packet Format (wire layout, big-endian)
//!
//! ```text
//! ┌──────────────────────────────────────────────┐
//! │ Header (4 B): version(1) | type(1) | id(2)  │  ← associated data
//! │ Sequence Counter (4 B, big-endian u32)       │  ← associated data
//! │ Nonce (16 B): counter_hi(8) | uid_hash(4)   │  ← plaintext (transmitted)
//! │               | zeros(4)                     │
//! │ Ciphertext (N B, N = payload length)         │
//! │ Authentication Tag (16 B)                    │
//! └──────────────────────────────────────────────┘
//! Total overhead: 4 + 4 + 16 + 16 = 40 bytes
//! ```
//!
//! ## Design Rationale
//! - Nonce = 64-bit sequence counter (prevents reuse) ‖ 32-bit device UID hash
//!   ‖ 32-bit zero-padding. Full 128-bit nonce fed to ASCON-128.
//! - Associated data = header ‖ sequence counter (authenticated, not encrypted).
//!   Modifying either field causes tag verification failure.
//! - Replay protection: receiver rejects packets with sequence ≤ last accepted.

#![no_std]
#![forbid(unsafe_code)]

use rustguard_core::{ascon_aead_encrypt, ascon_aead_decrypt, ascon_hash};
use heapless::Vec;

// ── Constants ─────────────────────────────────────────────────────────────────

pub const HEADER_LEN:   usize = 4;   // version(1) + type(1) + device_id(2)
pub const SEQ_LEN:      usize = 4;   // u32 sequence counter
pub const NONCE_LEN:    usize = 16;  // 128-bit ASCON nonce
pub const TAG_LEN:      usize = 16;  // 128-bit ASCON tag
pub const OVERHEAD:     usize = HEADER_LEN + SEQ_LEN + NONCE_LEN + TAG_LEN; // 40
pub const MAX_PAYLOAD:  usize = 512;
pub const MAX_PACKET:   usize = OVERHEAD + MAX_PAYLOAD; // 552

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PacketError {
    /// Packet is shorter than the minimum overhead.
    InvalidSize,
    /// Received sequence counter is not greater than the last accepted value.
    ReplayDetected,
    /// ASCON-128 tag verification failed — packet is corrupted or forged.
    AuthenticationFailed,
    /// Output buffer is too small to hold the plaintext.
    BufferTooSmall,
}

// ── PacketBuilder ─────────────────────────────────────────────────────────────

/// Builds and validates RustGuard-PAP packets.
///
/// The pre-shared 128-bit key should be provisioned once per device
/// and stored in non-volatile memory.
pub struct PacketBuilder {
    key:              [u8; 16],
    sequence_counter: u32,
}

impl PacketBuilder {
    /// Create a new `PacketBuilder` with a given key and initial sequence number.
    ///
    /// `initial_sequence` should be 0 for a freshly provisioned device,
    /// or the last stored counter value for a device recovering from power loss.
    pub fn new(key: [u8; 16], initial_sequence: u32) -> Self {
        Self { key, sequence_counter: initial_sequence }
    }

    /// Encrypt and authenticate a payload, returning the full PAP packet.
    ///
    /// The sequence counter is incremented after every successful call.
    ///
    /// ## Arguments
    /// - `payload`     : Raw sensor data to encrypt (≤ 512 bytes)
    /// - `device_id`   : 16-bit device identifier embedded in the header
    /// - `version`     : Protocol version byte (use `1`)
    /// - `device_type` : Device category byte (application-defined)
    pub fn build_packet(
        &mut self,
        payload:     &[u8],
        device_id:   u16,
        version:     u8,
        device_type: u8,
    ) -> Vec<u8, MAX_PACKET> {
        // Build header (4 bytes)
        let mut header = [0u8; HEADER_LEN];
        header[0] = version;
        header[1] = device_type;
        header[2..4].copy_from_slice(&device_id.to_be_bytes());

        // Sequence counter (4 bytes, big-endian)
        let seq = self.sequence_counter;
        self.sequence_counter = self.sequence_counter.wrapping_add(1);
        let seq_bytes = seq.to_be_bytes();

        // Build 128-bit nonce:
        //   [0..8]  = 64-bit sequence counter (extended to u64, big-endian)
        //   [8..12] = first 4 bytes of ASCON-HASH(device_id_bytes)
        //   [12..16] = 0x00000000 (deterministic padding)
        let mut nonce = [0u8; NONCE_LEN];
        nonce[0..8].copy_from_slice(&(seq as u64).to_be_bytes());
        let mut uid_hash = [0u8; 32];
        ascon_hash(&device_id.to_be_bytes(), &mut uid_hash);
        nonce[8..12].copy_from_slice(&uid_hash[0..4]);
        // nonce[12..16] left as 0x00000000

        // Associated data = header ‖ sequence counter
        let mut ad = [0u8; HEADER_LEN + SEQ_LEN];
        ad[0..4].copy_from_slice(&header);
        ad[4..8].copy_from_slice(&seq_bytes);

        // Encrypt
        let mut ciphertext_buf = [0u8; MAX_PAYLOAD];
        let mut tag = [0u8; TAG_LEN];
        let ct = &mut ciphertext_buf[..payload.len()];
        ascon_aead_encrypt(&self.key, &nonce, &ad, payload, ct, &mut tag);

        // Assemble packet: header | seq | nonce | ciphertext | tag
        let mut packet: Vec<u8, MAX_PACKET> = Vec::new();
        packet.extend_from_slice(&header).ok();
        packet.extend_from_slice(&seq_bytes).ok();
        packet.extend_from_slice(&nonce).ok();
        packet.extend_from_slice(ct).ok();
        packet.extend_from_slice(&tag).ok();
        packet
    }

    /// Authenticate and decrypt a received PAP packet.
    ///
    /// Returns the number of plaintext bytes written to `payload_out`.
    ///
    /// ## Replay Protection
    /// The caller must pass the last accepted sequence number as
    /// `expected_min_seq`. Any packet with sequence ≤ this value is rejected.
    pub fn unwrap_packet(
        &self,
        packet:          &[u8],
        expected_min_seq: u32,
        payload_out:     &mut [u8],
    ) -> Result<usize, PacketError> {
        if packet.len() < OVERHEAD {
            return Err(PacketError::InvalidSize);
        }

        // Parse fields
        let header   = &packet[0..HEADER_LEN];
        let seq_bytes: [u8; 4] = packet[HEADER_LEN..HEADER_LEN + SEQ_LEN].try_into().unwrap();
        let seq      = u32::from_be_bytes(seq_bytes);
        let nonce: [u8; NONCE_LEN] = packet[HEADER_LEN + SEQ_LEN..HEADER_LEN + SEQ_LEN + NONCE_LEN]
            .try_into()
            .unwrap();

        let payload_len = packet.len() - OVERHEAD;
        let ct_start    = HEADER_LEN + SEQ_LEN + NONCE_LEN;
        let ciphertext  = &packet[ct_start..ct_start + payload_len];
        let tag: [u8; TAG_LEN] = packet[ct_start + payload_len..].try_into().unwrap();

        // Replay check: reject if seq is not strictly greater than expected_min_seq
        // expected_min_seq = 0 means we accept seq >= 1 (first packet)
        // expected_min_seq = N means we only accept seq > N
        if seq <= expected_min_seq && !(expected_min_seq == 0 && seq == 0) {
            // Allow seq == 0 only when expected_min_seq == 0 (first packet from a fresh device)
            return Err(PacketError::ReplayDetected);
        }

        if payload_out.len() < payload_len {
            return Err(PacketError::BufferTooSmall);
        }

        // Reconstruct AD
        let mut ad = [0u8; HEADER_LEN + SEQ_LEN];
        ad[0..4].copy_from_slice(header);
        ad[4..8].copy_from_slice(&seq_bytes);

        let ok = ascon_aead_decrypt(
            &self.key,
            &nonce,
            &ad,
            ciphertext,
            &mut payload_out[..payload_len],
            &tag,
        );

        if ok { Ok(payload_len) } else { Err(PacketError::AuthenticationFailed) }
    }
}
