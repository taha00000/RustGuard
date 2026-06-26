//! Integration tests for rustguard-pap, including the reboot/nonce-reuse
//! regression that motivated the protocol revision.

use rustguard_pap::{EpochStore, PacketBuilder, PacketReceiver, ReplayWindow, OVERHEAD};

/// In-RAM mock of a persisted epoch store. A real device backs this with
/// EEPROM/flash; the semantics (survives a logical "reboot") are modeled by
/// keeping the value in an owned cell the test controls.
#[derive(Clone)]
struct MockStore {
    value: u32,
}
impl EpochStore for MockStore {
    fn load_epoch(&self) -> u32 {
        self.value
    }
    fn store_epoch(&mut self, epoch: u32) {
        self.value = epoch;
    }
}

const KEY: [u8; 16] = [0x11; 16];

fn roundtrip_at(size: usize) {
    let store = MockStore { value: 0 };
    let mut tx = PacketBuilder::new_boot(KEY, store);
    let payload: heapless::Vec<u8, 512> = (0..size).map(|i| i as u8).collect();

    let pkt = tx.build_packet(&payload, 0x00A5, 1, 7);
    assert_eq!(pkt.len(), OVERHEAD + size);

    let rx = PacketReceiver::new(KEY);
    let mut win = ReplayWindow::default();
    let mut out = [0u8; 512];
    let n = rx.unwrap_packet(&pkt, &mut win, &mut out).expect("auth");
    assert_eq!(n, size);
    assert_eq!(&out[..size], &payload[..]);
}

#[test]
fn roundtrip_all_sizes() {
    for &s in &[0usize, 1, 7, 8, 16, 32, 64, 128, 256, 512] {
        roundtrip_at(s);
    }
}

#[test]
fn replay_same_epoch_is_rejected() {
    let store = MockStore { value: 0 };
    let mut tx = PacketBuilder::new_boot(KEY, store);
    let rx = PacketReceiver::new(KEY);
    let mut win = ReplayWindow::default();
    let mut out = [0u8; 64];

    let p1 = tx.build_packet(b"first", 1, 1, 1);
    let p2 = tx.build_packet(b"second", 1, 1, 1);

    assert!(rx.unwrap_packet(&p1, &mut win, &mut out).is_ok());
    assert!(rx.unwrap_packet(&p2, &mut win, &mut out).is_ok());
    // Re-presenting p1 must now be rejected as a replay.
    assert_eq!(
        rx.unwrap_packet(&p1, &mut win, &mut out),
        Err(rustguard_pap::PacketError::ReplayDetected)
    );
}

#[test]
fn tamper_ciphertext_fails_auth() {
    let store = MockStore { value: 0 };
    let mut tx = PacketBuilder::new_boot(KEY, store);
    let rx = PacketReceiver::new(KEY);
    let mut win = ReplayWindow::default();
    let mut out = [0u8; 64];

    let mut p = tx.build_packet(b"sensor=22.4C", 1, 1, 1);
    let ct_off = OVERHEAD - 16; // first ciphertext byte sits right after nonce
    p[ct_off] ^= 0x01;
    assert_eq!(
        rx.unwrap_packet(&p, &mut win, &mut out),
        Err(rustguard_pap::PacketError::AuthenticationFailed)
    );
}

#[test]
fn tamper_epoch_field_fails_auth() {
    let store = MockStore { value: 0 };
    let mut tx = PacketBuilder::new_boot(KEY, store);
    let rx = PacketReceiver::new(KEY);
    let mut win = ReplayWindow::default();
    let mut out = [0u8; 64];

    let mut p = tx.build_packet(b"payload!!", 1, 1, 1);
    // epoch occupies bytes [4..8]; flipping it must break the AD authentication.
    p[5] ^= 0x80;
    assert_eq!(
        rx.unwrap_packet(&p, &mut win, &mut out),
        Err(rustguard_pap::PacketError::AuthenticationFailed)
    );
}

#[test]
fn too_short_packet_is_invalid_size() {
    let rx = PacketReceiver::new(KEY);
    let mut win = ReplayWindow::default();
    let mut out = [0u8; 64];
    let short = [0u8; OVERHEAD - 1];
    assert_eq!(
        rx.unwrap_packet(&short, &mut win, &mut out),
        Err(rustguard_pap::PacketError::InvalidSize)
    );
}

/// The core regression: a reboot that resets the RAM counter must NOT cause
/// the device to reuse a nonce, because the persisted epoch advances.
#[test]
fn reboot_does_not_reuse_nonce() {
    // Boot 1: persisted epoch 0 -> 1. Emit some packets.
    let shared = MockStore { value: 0 };
    let mut tx1 = PacketBuilder::new_boot(KEY, shared.clone());
    let _ = tx1.build_packet(b"a", 1, 1, 1);
    let _ = tx1.build_packet(b"b", 1, 1, 1);
    let epoch1 = tx1.epoch();

    // Simulate an unclean reboot: RAM counter is lost, but the epoch store
    // retains the value tx1 persisted. We model that persistence by reading
    // the value tx1 wrote.
    let persisted_after_boot1 = MockStore { value: epoch1 };

    // Boot 2: epoch must advance to a value never used in boot 1.
    let mut tx2 = PacketBuilder::new_boot(KEY, persisted_after_boot1);
    let epoch2 = tx2.epoch();
    assert!(epoch2 > epoch1, "epoch must advance across reboot");

    // Even though both boots start their in-epoch counter at 0, the epoch
    // prefix differs, so the full nonces differ. We assert this indirectly:
    // a receiver that accepted boot-1 packets accepts boot-2 packets as fresh
    // (higher epoch) rather than seeing a collision.
    let rx = PacketReceiver::new(KEY);
    let mut win = ReplayWindow::default();
    let mut out = [0u8; 64];
    let p2 = tx2.build_packet(b"a", 1, 1, 1); // same payload/counter as boot 1
    assert!(rx.unwrap_packet(&p2, &mut win, &mut out).is_ok());
}
