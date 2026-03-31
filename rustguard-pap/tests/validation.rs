use rustguard_pap::{PacketBuilder, PacketError};
use rustguard_core::{ascon_p, State};

#[test]
fn test_ascon_permutation() {
    let mut s = State { x0: 1, x1: 2, x2: 3, x3: 4, x4: 5 };
    ascon_p(&mut s, 12);
    assert_ne!(s.x0, 1);
}

#[test]
fn test_pap_build_and_unwrap() {
    let key = [0x42; 16];
    let mut builder = PacketBuilder::new(key, 0);
    
    let payload = b"Hello, Cortex-M! This is a 32-byte IoT payload.";
    let packet = builder.build_packet(payload.as_bytes(), 0x1234, 1, 2);
    
    assert_eq!(packet.len(), payload.len() + 36);
    
    let rx_builder = PacketBuilder::new(key, 0);
    let mut out_payload = [0u8; 512];
    
    let len = rx_builder.unwrap_packet(&packet, &mut out_payload).unwrap();
    assert_eq!(len, payload.len());
    assert_eq!(&out_payload[..len], payload.as_bytes());
}

#[test]
fn test_pap_tamper() {
    let key = [0x42; 16];
    let mut builder = PacketBuilder::new(key, 0);
    let payload = b"Sensitive Sensor Data";
    let mut packet = builder.build_packet(payload, 0x1234, 1, 2);
    
    packet[30] ^= 1;
    
    let rx_builder = PacketBuilder::new(key, 0);
    let mut out_payload = [0u8; 512];
    let res = rx_builder.unwrap_packet(&packet, &mut out_payload);
    assert_eq!(res, Err(PacketError::AuthenticationFailed));
}
