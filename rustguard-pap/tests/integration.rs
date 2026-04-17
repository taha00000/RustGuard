use rustguard_pap::{PacketBuilder, PacketError, OVERHEAD};

#[test]
fn test_build_and_unwrap_32byte_payload() {
    let key = [0x42u8; 16];
    let mut builder = PacketBuilder::new(key, 0);
    let payload = b"Environmental Sensor Temp: 22.4C"; // exactly 32 bytes
    assert_eq!(payload.len(), 32);

    let packet = builder.build_packet(payload, 0x1011, 1, 1);
    assert_eq!(packet.len(), OVERHEAD + payload.len(), "packet length must be overhead + payload");

    let rx = PacketBuilder::new(key, 0);
    let mut out = [0u8; 512];
    let len = rx.unwrap_packet(&packet, 0, &mut out).expect("must decrypt successfully");
    assert_eq!(len, payload.len());
    assert_eq!(&out[..len], payload.as_slice());
}

#[test]
fn test_replay_detection() {
    let key = [0x10u8; 16];
    let mut builder = PacketBuilder::new(key, 5); // starts at seq 5
    let payload = b"data";
    let packet = builder.build_packet(payload, 0x0001, 1, 1);

    let rx = PacketBuilder::new(key, 0);
    // First accept with expected_min = 4 (seq 5 > 4 → OK)
    let mut out = [0u8; 512];
    let res = rx.unwrap_packet(&packet, 4, &mut out);
    assert!(res.is_ok(), "seq 5 > 4 should pass");

    // Now reject with expected_min = 5 (seq 5 ≤ 5 → replay)
    let res2 = rx.unwrap_packet(&packet, 5, &mut out);
    assert_eq!(res2, Err(PacketError::ReplayDetected), "seq 5 ≤ 5 should be rejected");
}

#[test]
fn test_tamper_ciphertext_rejected() {
    let key = [0xAAu8; 16];
    let mut builder = PacketBuilder::new(key, 0);
    let payload = b"Sensitive Sensor Data";
    let mut packet = builder.build_packet(payload, 0x1234, 1, 2);

    // Flip one bit in the ciphertext portion
    let ct_offset = 4 + 4 + 16; // header + seq + nonce
    packet[ct_offset] ^= 0x01;

    let rx = PacketBuilder::new(key, 0);
    let mut out = [0u8; 512];
    let res = rx.unwrap_packet(&packet, 0, &mut out);
    assert_eq!(res, Err(PacketError::AuthenticationFailed));
}

#[test]
fn test_tamper_header_rejected() {
    let key = [0xBBu8; 16];
    let mut builder = PacketBuilder::new(key, 0);
    let payload = b"sensor reading";
    let mut packet = builder.build_packet(payload, 0x0001, 1, 1);

    // Modify device_type in header (byte 1)
    packet[1] ^= 0xFF;

    let rx = PacketBuilder::new(key, 0);
    let mut out = [0u8; 512];
    let res = rx.unwrap_packet(&packet, 0, &mut out);
    assert_eq!(res, Err(PacketError::AuthenticationFailed));
}

#[test]
fn test_tamper_tag_rejected() {
    let key = [0xCCu8; 16];
    let mut builder = PacketBuilder::new(key, 0);
    let payload = b"reading";
    let mut packet = builder.build_packet(payload, 0x0002, 1, 1);

    // Corrupt last byte of tag
    let last = packet.len() - 1;
    packet[last] ^= 0xFF;

    let rx = PacketBuilder::new(key, 0);
    let mut out = [0u8; 512];
    let res = rx.unwrap_packet(&packet, 0, &mut out);
    assert_eq!(res, Err(PacketError::AuthenticationFailed));
}

#[test]
fn test_sequence_counter_increments() {
    let key = [0x01u8; 16];
    let mut builder = PacketBuilder::new(key, 0);
    let payload = b"ping";

    let p1 = builder.build_packet(payload, 0x0001, 1, 1);
    let p2 = builder.build_packet(payload, 0x0001, 1, 1);
    let p3 = builder.build_packet(payload, 0x0001, 1, 1);

    // Sequences 1, 2, 3 → all different nonces → different ciphertexts
    let ct_off = 4 + 4 + 16;
    let ct_end = ct_off + payload.len();
    assert_ne!(&p1[ct_off..ct_end], &p2[ct_off..ct_end], "different seqs → different CT");
    assert_ne!(&p2[ct_off..ct_end], &p3[ct_off..ct_end]);
}

#[test]
fn test_variable_payload_sizes() {
    let key = [0x77u8; 16];
    for size in [8, 16, 32, 64, 128, 256, 512] {
        let payload = vec![0xABu8; size];
        let mut builder = PacketBuilder::new(key, 0);
        let packet = builder.build_packet(&payload, 0x0001, 1, 1);
        assert_eq!(packet.len(), OVERHEAD + size, "size={}", size);
        let rx = PacketBuilder::new(key, 0);
        let mut out = vec![0u8; size];
        let len = rx.unwrap_packet(&packet, 0, &mut out).expect(&format!("decrypt size={}", size));
        assert_eq!(len, size);
        assert_eq!(&out[..len], &payload[..]);
    }
}

#[test]
fn test_wrong_key_fails() {
    let key_tx = [0x11u8; 16];
    let key_rx = [0x22u8; 16]; // different key
    let mut builder = PacketBuilder::new(key_tx, 0);
    let payload = b"secret";
    let packet = builder.build_packet(payload, 0x0001, 1, 1);

    let rx = PacketBuilder::new(key_rx, 0);
    let mut out = [0u8; 512];
    let res = rx.unwrap_packet(&packet, 0, &mut out);
    assert_eq!(res, Err(PacketError::AuthenticationFailed));
}

#[test]
fn test_minimum_packet_size_check() {
    let key = [0x99u8; 16];
    let rx = PacketBuilder::new(key, 0);
    let mut out = [0u8; 512];
    // Packet of 39 bytes (1 less than OVERHEAD=40) must fail with InvalidSize
    let short = vec![0u8; OVERHEAD - 1];
    let res = rx.unwrap_packet(&short, 0, &mut out);
    assert_eq!(res, Err(PacketError::InvalidSize));
}
