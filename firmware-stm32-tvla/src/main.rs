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

mod board {
    //! Thin MMIO layer for the STM32F303RCT on the CW308 (CW308T-STM32F3).
    //! Isolated `unsafe` for register access only; the crypto crates remain
    //! `forbid(unsafe_code)`. All register addresses/bit positions are from the
    //! STM32F3 reference manual RM0316.
    //!
    //! ## Board-specific values — verify on first bring-up
    //! These three are the standard CW308T-STM32F3 wiring. If your target board
    //! revision differs, these are the only constants to change:
    //!   * UART     : USART2 on PA2 (TX) / PA3 (RX), AF7   [`CW308 J1 UART hdr`]
    //!   * trigger  : PA12 -> CW308 GPIO4/tio4 trigger line
    //!   * baud     : 38400 8N1 (ChipWhisperer default; matches capture script)
    //! Bring-up checklist is in docs/hardware_setup.md: confirm the 'k'->'z' ack
    //! over UART, then scope the trigger pin, before trusting any capture.
    //!
    //! ## Clock
    //! Runs on the internal HSI (8 MHz), which is always present, so the UART and
    //! protocol come up the instant you flash — no dependency on an external
    //! clock for first bring-up. For the *synchronous* capture that gives the
    //! cleanest first-order TVLA (CW `clkgen_x4`), feed the CW-provided clock to
    //! the target and switch to HSE-bypass; see CLOCK_HZ below and the runbook.

    #[inline(always)]
    pub fn rd(a: u32) -> u32 {
        unsafe { core::ptr::read_volatile(a as *const u32) }
    }
    #[inline(always)]
    pub fn wr(a: u32, v: u32) {
        unsafe { core::ptr::write_volatile(a as *mut u32, v) }
    }

    // ── RM0316 register map ──────────────────────────────────────────────────
    const RCC_AHBENR: u32 = 0x4002_1014; // GPIO port clocks live on AHB
    const RCC_APB1ENR: u32 = 0x4002_101C; // USART2 clock lives on APB1
    const RCC_AHBENR_IOPAEN: u32 = 1 << 17; // GPIOA clock enable
    const RCC_APB1ENR_USART2EN: u32 = 1 << 17; // USART2 clock enable

    const GPIOA_MODER: u32 = 0x4800_0000;
    const GPIOA_AFRL: u32 = 0x4800_0020; // alt-function low (pins 0..7)
    const GPIOA_BSRR: u32 = 0x4800_0018; // atomic set/reset

    const USART2_BRR: u32 = 0x4000_440C;
    const USART2_CR1: u32 = 0x4000_4400;
    const USART_ISR: u32 = 0x4000_441C;
    const USART_RDR: u32 = 0x4000_4424;
    const USART_TDR: u32 = 0x4000_4428;
    const USART_ISR_RXNE: u32 = 1 << 5;
    const USART_ISR_TXE: u32 = 1 << 7;
    const USART_CR1_UE: u32 = 1 << 0;
    const USART_CR1_RE: u32 = 1 << 2;
    const USART_CR1_TE: u32 = 1 << 3;

    // ── Board-specific (see module doc) ──────────────────────────────────────
    /// HSI default. Set to the CW clkgen frequency (e.g. 7_370_000) if you move
    /// the target onto the external CW clock for synchronous capture.
    pub const CLOCK_HZ: u32 = 8_000_000;
    pub const BAUD: u32 = 38_400;
    const TRIG_PIN: u32 = 1 << 12; // PA12

    pub fn trigger_high() {
        wr(GPIOA_BSRR, TRIG_PIN);
    }
    pub fn trigger_low() {
        wr(GPIOA_BSRR, TRIG_PIN << 16); // upper half-word = reset
    }

    pub fn putc(b: u8) {
        while rd(USART_ISR) & USART_ISR_TXE == 0 {}
        wr(USART_TDR, b as u32);
    }
    pub fn getc() -> u8 {
        while rd(USART_ISR) & USART_ISR_RXNE == 0 {}
        rd(USART_RDR) as u8
    }

    /// Bring up GPIOA + USART2 and the trigger pin. HSI is already running after
    /// reset (sysclk = HSI 8 MHz, AHB/APB1 prescalers = /1, so PCLK1 = 8 MHz), so
    /// we only enable the peripheral clocks, mux the pins, and configure USART2.
    pub fn clocks_uart_init() {
        // Peripheral clocks.
        wr(RCC_AHBENR, rd(RCC_AHBENR) | RCC_AHBENR_IOPAEN);
        wr(RCC_APB1ENR, rd(RCC_APB1ENR) | RCC_APB1ENR_USART2EN);

        // PA2,PA3 -> alternate function (0b10); PA12 -> general output (0b01).
        let mut moder = rd(GPIOA_MODER);
        moder &= !((0b11 << 4) | (0b11 << 6) | (0b11 << 24)); // clear PA2/PA3/PA12
        moder |= (0b10 << 4) | (0b10 << 6) | (0b01 << 24);
        wr(GPIOA_MODER, moder);

        // PA2,PA3 alternate function = AF7 (USART2 TX/RX).
        let mut afrl = rd(GPIOA_AFRL);
        afrl &= !((0xF << 8) | (0xF << 12));
        afrl |= (7 << 8) | (7 << 12);
        wr(GPIOA_AFRL, afrl);

        // Baud: BRR = f_CK / baud (USART2 oversampling-by-16, integer divisor).
        wr(USART2_BRR, CLOCK_HZ / BAUD);
        // Enable USART2: UE | TE | RE. 8N1 is the reset default (M=0, no parity).
        wr(USART2_CR1, USART_CR1_UE | USART_CR1_TE | USART_CR1_RE);
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
