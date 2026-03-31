use rustguard_core::{ascon_aead_encrypt, ascon_aead_decrypt, ascon_hash};
use heapless::Vec;

pub struct PacketBuilder {
    key: [u8; 16],
    sequence_counter: u32,
}

#[derive(Debug, PartialEq)]
pub enum PacketError {
    InvalidSize,
    ReplayDetected,
    AuthenticationFailed,
}

impl PacketBuilder {
    pub const MAX_PACKET_SIZE: usize = 512 + 36; // 512 payload max

    pub fn new(key: [u8; 16], initial_sequence: u32) -> Self {
        Self {
            key,
            sequence_counter: initial_sequence,
        }
    }

    pub fn unwrap_packet(&self, packet: &[u8], payload_out: &mut [u8]) -> Result<usize, PacketError> {
        if packet.len() < 36 {
            return Err(PacketError::InvalidSize);
        }

        let seq_bytes: [u8; 4] = packet[4..8].try_into().unwrap();
        let seq = u32::from_be_bytes(seq_bytes);

        if seq < self.sequence_counter {
            // Very simple replay window
            return Err(PacketError::ReplayDetected);
        }

        let mut nonce = [0u8; 16];
        nonce[0..12].copy_from_slice(&packet[8..20]);
        nonce[12..16].fill(0);

        let assoc_data = &packet[0..8];
        let payload_len = packet.len() - 36;
        let ciphertext = &packet[20..20 + payload_len];
        
        // Use copy to un-reference
        let mut tag = [0u8; 16];
        tag.copy_from_slice(&packet[20 + payload_len..]);

        let success = ascon_aead_decrypt(
            &self.key,
            &nonce,
            assoc_data,
            ciphertext,
            &mut payload_out[..payload_len],
            &tag,
        );

        if success {
            Ok(payload_len)
        } else {
            Err(PacketError::AuthenticationFailed)
        }
    }

    pub fn build_packet(&mut self, payload: &[u8], device_id: u16, version: u8, device_type: u8) -> Vec<u8, {PacketBuilder::MAX_PACKET_SIZE}> {
        let seq = self.sequence_counter;
        self.sequence_counter += 1;

        let mut output = Vec::new();

        // Header: Version (1), Type (1), ID (2)
        output.push(version).unwrap();
        output.push(device_type).unwrap();
        output.extend_from_slice(&device_id.to_be_bytes()).unwrap();

        // Sequence
        output.extend_from_slice(&seq.to_be_bytes()).unwrap();

        // Nonce (Hardware Timer + ASCON-HASH of UID)
        let mut nonce = [0u8; 16];
        nonce[0..8].copy_from_slice(&(seq as u64).to_be_bytes()); // using seq as hardware timer in library
        
        let mut uid_hash = [0u8; 32];
        ascon_hash(&device_id.to_be_bytes(), &mut uid_hash);
        
        nonce[8..12].copy_from_slice(&uid_hash[0..4]);
        nonce[12..16].fill(0);

        output.extend_from_slice(&nonce[0..12]).unwrap();

        let mut ciphertext_buf = [0u8; 512];
        let mut tag = [0u8; 16];
        
        let ciphertext = &mut ciphertext_buf[..payload.len()];

        ascon_aead_encrypt(
            &self.key,
            &nonce,
            &output[0..8], // AD is header + sequence
            payload,
            ciphertext,
            &mut tag,
        );

        output.extend_from_slice(ciphertext).unwrap();
        output.extend_from_slice(&tag).unwrap();

        output
    }
}
