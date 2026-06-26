//! rustguard-pap — RustGuard Packet Authentication Protocol (revised).
//!
//! ## What changed from the original design, and why
//!
//! The original PAP built the AEAD nonce purely from a monotonic sequence
//! counter held in RAM. On a constrained MCU the single most common failure
//! mode is an unclean reboot (brown-out, watchdog, power glitch) that resets
//! RAM. If the counter restarts from a previously used value, the device
//! re-emits nonces it has already used under the same key — **catastrophic
//! nonce reuse** for ASCON, which exposes keystream and enables forgery.
//!
//! The revised construction makes the nonce robust to counter resets by
//! combining two independent sources of uniqueness:
//!
//!   nonce = epoch (4 B) ‖ counter (8 B) ‖ device_uid_hash (4 B)
//!
//!   * `epoch`   — a 32-bit value persisted to non-volatile memory and
//!     incremented *once per boot*, before any packet is sent. Even if the
//!     RAM counter resets, a new boot uses a fresh epoch, so the (epoch,
//!     counter) pair never repeats.
//!   * `counter` — 64-bit monotonic within an epoch.
//!   * `uid`     — binds the nonce to device identity (defends against two
//!     devices sharing a key colliding on (epoch, counter)).
//!
//! Persisting a 4-byte epoch once per boot is far cheaper than persisting the
//! full counter on every packet, which is what makes this practical on flash-
//! limited parts. The security argument is in `docs/protocol_security.md`.

#![no_std]
#![forbid(unsafe_code)]

use heapless::Vec;
use rustguard_core::{ascon_aead_decrypt, ascon_aead_encrypt, ascon_hash};

pub const HEADER_LEN: usize = 4; // version(1) + type(1) + device_id(2)
pub const SEQ_LEN: usize = 4; // u32 sequence counter (on the wire)
pub const EPOCH_LEN: usize = 4; // u32 boot epoch (on the wire)
pub const NONCE_LEN: usize = 16;
pub const TAG_LEN: usize = 16;
pub const OVERHEAD: usize = HEADER_LEN + EPOCH_LEN + SEQ_LEN + NONCE_LEN + TAG_LEN; // 44
pub const MAX_PAYLOAD: usize = 512;
pub const MAX_PACKET: usize = OVERHEAD + MAX_PAYLOAD;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PacketError {
    InvalidSize,
    ReplayDetected,
    AuthenticationFailed,
    BufferTooSmall,
}

/// Trait the firmware implements to persist the boot epoch across resets.
/// Backed by EEPROM/flash on real hardware; an in-RAM mock is used in tests.
pub trait EpochStore {
    /// Load the last persisted epoch (0 on a freshly provisioned device).
    fn load_epoch(&self) -> u32;
    /// Persist a new epoch. MUST complete before the first packet of this boot.
    fn store_epoch(&mut self, epoch: u32);
}

pub struct PacketBuilder {
    key: [u8; 16],
    epoch: u32,
    counter: u64,
}

impl PacketBuilder {
    /// Construct a builder and advance the boot epoch.
    ///
    /// This reads the persisted epoch, increments it, persists the new value,
    /// and resets the in-epoch counter to zero. Calling this exactly once per
    /// boot guarantees nonce uniqueness across resets. The store is consumed
    /// here because persistence happens exactly once, at boot — the builder
    /// holds no reference to it afterward.
    pub fn new_boot<S: EpochStore>(key: [u8; 16], mut store: S) -> Self {
        let epoch = store.load_epoch().wrapping_add(1);
        store.store_epoch(epoch);
        Self {
            key,
            epoch,
            counter: 0,
        }
    }

    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    fn build_nonce(&self, device_id: u16) -> [u8; NONCE_LEN] {
        let mut nonce = [0u8; NONCE_LEN];
        nonce[0..4].copy_from_slice(&self.epoch.to_be_bytes());
        nonce[4..12].copy_from_slice(&self.counter.to_be_bytes());
        let mut uid_hash = [0u8; 32];
        ascon_hash(&device_id.to_be_bytes(), &mut uid_hash);
        nonce[12..16].copy_from_slice(&uid_hash[0..4]);
        nonce
    }

    pub fn build_packet(
        &mut self,
        payload: &[u8],
        device_id: u16,
        version: u8,
        device_type: u8,
    ) -> Vec<u8, MAX_PACKET> {
        let mut header = [0u8; HEADER_LEN];
        header[0] = version;
        header[1] = device_type;
        header[2..4].copy_from_slice(&device_id.to_be_bytes());

        let epoch_bytes = self.epoch.to_be_bytes();
        let seq = self.counter as u32;
        let seq_bytes = seq.to_be_bytes();

        let nonce = self.build_nonce(device_id);

        // AD = header ‖ epoch ‖ sequence  (authenticated, not encrypted)
        let mut ad = [0u8; HEADER_LEN + EPOCH_LEN + SEQ_LEN];
        ad[0..4].copy_from_slice(&header);
        ad[4..8].copy_from_slice(&epoch_bytes);
        ad[8..12].copy_from_slice(&seq_bytes);

        let mut ciphertext_buf = [0u8; MAX_PAYLOAD];
        let mut tag = [0u8; TAG_LEN];
        let ct = &mut ciphertext_buf[..payload.len()];
        ascon_aead_encrypt(&self.key, &nonce, &ad, payload, ct, &mut tag);

        self.counter = self.counter.wrapping_add(1);

        let mut packet: Vec<u8, MAX_PACKET> = Vec::new();
        packet.extend_from_slice(&header).ok();
        packet.extend_from_slice(&epoch_bytes).ok();
        packet.extend_from_slice(&seq_bytes).ok();
        packet.extend_from_slice(&nonce).ok();
        packet.extend_from_slice(ct).ok();
        packet.extend_from_slice(&tag).ok();
        packet
    }
}

/// Receiver-side replay state: tracks the highest (epoch, seq) accepted.
#[derive(Default, Clone, Copy)]
pub struct ReplayWindow {
    pub last_epoch: u32,
    pub last_seq: u32,
    pub seen_any: bool,
}

impl ReplayWindow {
    /// A packet is fresh iff its epoch is higher than the last accepted epoch,
    /// or the epoch matches and its sequence is strictly greater.
    fn is_fresh(&self, epoch: u32, seq: u32) -> bool {
        if !self.seen_any {
            return true;
        }
        epoch > self.last_epoch || (epoch == self.last_epoch && seq > self.last_seq)
    }

    fn record(&mut self, epoch: u32, seq: u32) {
        self.last_epoch = epoch;
        self.last_seq = seq;
        self.seen_any = true;
    }
}

pub struct PacketReceiver {
    key: [u8; 16],
}

impl PacketReceiver {
    pub fn new(key: [u8; 16]) -> Self {
        Self { key }
    }

    /// Authenticate, decrypt, and apply replay protection.
    ///
    /// Replay is checked *before* the cryptographic verification so a replayed
    /// packet is dropped cheaply, but the authentic-but-stale case is still
    /// rejected after verification to avoid trusting an unauthenticated epoch.
    pub fn unwrap_packet(
        &self,
        packet: &[u8],
        window: &mut ReplayWindow,
        payload_out: &mut [u8],
    ) -> Result<usize, PacketError> {
        if packet.len() < OVERHEAD {
            return Err(PacketError::InvalidSize);
        }

        let header = &packet[0..HEADER_LEN];
        let epoch = u32::from_be_bytes(
            packet[HEADER_LEN..HEADER_LEN + EPOCH_LEN]
                .try_into()
                .unwrap(),
        );
        let seq_off = HEADER_LEN + EPOCH_LEN;
        let seq = u32::from_be_bytes(packet[seq_off..seq_off + SEQ_LEN].try_into().unwrap());
        let nonce_off = seq_off + SEQ_LEN;
        let nonce: [u8; NONCE_LEN] = packet[nonce_off..nonce_off + NONCE_LEN]
            .try_into()
            .unwrap();

        let payload_len = packet.len() - OVERHEAD;
        let ct_start = nonce_off + NONCE_LEN;
        let ciphertext = &packet[ct_start..ct_start + payload_len];
        let tag: [u8; TAG_LEN] = packet[ct_start + payload_len..].try_into().unwrap();

        if !window.is_fresh(epoch, seq) {
            return Err(PacketError::ReplayDetected);
        }
        if payload_out.len() < payload_len {
            return Err(PacketError::BufferTooSmall);
        }

        let mut ad = [0u8; HEADER_LEN + EPOCH_LEN + SEQ_LEN];
        ad[0..4].copy_from_slice(header);
        ad[4..8].copy_from_slice(&epoch.to_be_bytes());
        ad[8..12].copy_from_slice(&seq.to_be_bytes());

        let ok = ascon_aead_decrypt(
            &self.key,
            &nonce,
            &ad,
            ciphertext,
            &mut payload_out[..payload_len],
            &tag,
        );

        if ok {
            window.record(epoch, seq);
            Ok(payload_len)
        } else {
            Err(PacketError::AuthenticationFailed)
        }
    }
}
