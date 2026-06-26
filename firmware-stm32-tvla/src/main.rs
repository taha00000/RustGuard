//! STM32F303 (CW308 target) TVLA firmware for RustGuard.
//!
//! This is the side-channel measurement target. It speaks a minimal
//! simpleserial-style protocol over UART so the ChipWhisperer host can drive
//! fixed-vs-random TVLA acquisition, and it raises a dedicated GPIO trigger
//! immediately around the cryptographic operation so power traces align to the
//! single instruction boundary (UART triggering is deliberately avoided — its
//! jitter destroys first-order TVLA).
//!
//! ## Protocol (one ASCII line per command, hex-encoded)
//!   'k' <32 hex>   set 16-byte key
//!   'p' <32 hex>   set 16-byte plaintext block, run AEAD encrypt under trigger
//!   'd' <...>      run AEAD *decrypt* of a fixed ciphertext under trigger
//!                  (used for the tag-comparison leakage experiment)
//! The host selects which decrypt variant is linked at build time:
//!   default build        -> constant-time tag check (the device under test)
//!   --features leaky      -> variable-time tag check (TVLA positive control)
//!
//! Build two binaries, capture both. If the pipeline cannot see the leaky
//! control, the "safe" result is not trustworthy.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use panic_halt as _;
use rustguard_core::ascon_aead_encrypt;

#[cfg(not(feature = "leaky"))]
use rustguard_core::ascon_aead_decrypt as aead_decrypt;
#[cfg(feature = "leaky")]
use rustguard_core::ascon_aead_decrypt_variabletime as aead_decrypt;

// NOTE: register addresses below target STM32F303RCT on the CW308. They are
// placeholders documented in docs/hardware_setup.md and must be confirmed
// against the actual board revision before the first capture run. The trigger
// pin is the CW308 standard GPIO4/trigger line.

mod board {
    //! Thin MMIO layer. Isolated `unsafe` for register access only; the crypto
    //! crates remain `forbid(unsafe_code)`.
    #[inline(always)]
    pub fn rd(a: u32) -> u32 {
        unsafe { core::ptr::read_volatile(a as *const u32) }
    }
    #[inline(always)]
    pub fn wr(a: u32, v: u32) {
        unsafe { core::ptr::write_volatile(a as *mut u32, v) }
    }

    // ── TODO(hardware): confirm these against the STM32F303 reference manual ──
    pub const TRIG_PORT_BSRR: u32 = 0x4800_0418; // GPIOA BSRR (set/reset)
    pub const TRIG_PIN: u32 = 1 << 12; // PA12 -> CW308 trigger (verify)

    pub const USART_TDR: u32 = 0x4000_4428;
    pub const USART_RDR: u32 = 0x4000_4424;
    pub const USART_ISR: u32 = 0x4000_441C;

    pub fn trigger_high() {
        wr(TRIG_PORT_BSRR, TRIG_PIN);
    }
    pub fn trigger_low() {
        wr(TRIG_PORT_BSRR, TRIG_PIN << 16);
    }

    pub fn putc(b: u8) {
        while rd(USART_ISR) & (1 << 7) == 0 {} // TXE
        wr(USART_TDR, b as u32);
    }
    pub fn getc() -> u8 {
        while rd(USART_ISR) & (1 << 5) == 0 {} // RXNE
        rd(USART_RDR) as u8
    }
    pub fn clocks_uart_init() {
        // TODO(hardware): enable GPIO + USART clocks, set baud (38400 8N1 is the
        // ChipWhisperer default), configure PA12 as output, USART pins as AF.
        // Left as a documented stub so the capture host wiring can be validated
        // against docs/hardware_setup.md before flashing.
    }
}

fn hexval(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

fn read_hex_16() -> [u8; 16] {
    let mut out = [0u8; 16];
    for byte in out.iter_mut() {
        let hi = hexval(board::getc());
        let lo = hexval(board::getc());
        *byte = (hi << 4) | lo;
    }
    out
}

fn send_hex(bytes: &[u8]) {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    for &b in bytes {
        board::putc(LUT[(b >> 4) as usize]);
        board::putc(LUT[(b & 0xf) as usize]);
    }
    board::putc(b'\n');
}

#[entry]
fn main() -> ! {
    board::clocks_uart_init();

    let mut key = [0u8; 16];
    let nonce = [0x00u8; 16]; // fixed nonce for the leakage experiment
    let ad = [0u8; 0];

    loop {
        let cmd = board::getc();
        match cmd {
            b'k' => {
                key = read_hex_16();
                board::putc(b'z');
                board::putc(b'\n');
            }
            b'p' => {
                let pt = read_hex_16();
                let mut ct = [0u8; 16];
                let mut tag = [0u8; 16];
                board::trigger_high();
                ascon_aead_encrypt(&key, &nonce, &ad, &pt, &mut ct, &mut tag);
                board::trigger_low();
                send_hex(&tag); // return tag so host can sanity-check
            }
            b'd' => {
                // Fixed valid ct/tag is recomputed here so the decrypt path
                // exercises the tag comparison on matching input; the host
                // varies the *key* class (fixed vs random) for TVLA.
                let pt = [0u8; 16];
                let mut ct = [0u8; 16];
                let mut tag = [0u8; 16];
                ascon_aead_encrypt(&key, &nonce, &ad, &pt, &mut ct, &mut tag);
                let mut rec = [0u8; 16];
                board::trigger_high();
                let ok = aead_decrypt(&key, &nonce, &ad, &ct, &mut rec, &tag);
                board::trigger_low();
                board::putc(if ok { b'1' } else { b'0' });
                board::putc(b'\n');
            }
            _ => {}
        }
    }
}
