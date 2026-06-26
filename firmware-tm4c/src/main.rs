//! TM4C123GH6PM performance benchmark firmware.
//!
//! Measures real ASCON-128 cycle counts via the DWT CYCCNT counter and streams
//! results over UART0. This is the *performance* half of the study; the
//! side-channel (TVLA) half runs on the STM32F3 target in `firmware-stm32-tvla`.
//!
//! ## UART capture (important — read before flashing)
//!
//! The previous iteration could not capture UART because of a Windows ICDI
//! virtual-COM driver conflict. The fix is to NOT rely on the ICDI COM port:
//! wire an external USB-UART dongle (FT232/CP2102) to PA1 (TX) -> dongle RX and
//! GND -> GND, and read it from the host at 115200 8N1. This bypasses the ICDI
//! driver entirely. See docs/hardware_setup.md.

#![no_std]
#![no_main]

use core::fmt::Write;
use cortex_m::peripheral::DWT;
use cortex_m_rt::entry;
use panic_halt as _;
use rustguard_core::{ascon_aead_decrypt, ascon_aead_encrypt, ascon_p, State};

// ── Register addresses (TM4C123, direct access; no HAL) ──────────────────────
const SYSCTL_RCGCGPIO: u32 = 0x400F_E608;
const SYSCTL_RCGCUART: u32 = 0x400F_E618;
const GPIOA_BASE: u32 = 0x4000_4000;
const UART0_DR: u32 = 0x4000_C000;
const UART0_FR: u32 = 0x4000_C018;
const UART0_IBRD: u32 = 0x4000_C024;
const UART0_FBRD: u32 = 0x4000_C028;
const UART0_LCRH: u32 = 0x4000_C02C;
const UART0_CTL: u32 = 0x4000_C030;
const UART0_CC: u32 = 0x4000_CFC8;

#[inline(always)]
fn rd(addr: u32) -> u32 {
    // SAFETY-FREE: volatile MMIO via cortex-m's provided helpers would need
    // `unsafe`; this firmware crate intentionally allows unsafe ONLY for MMIO,
    // isolated here. The cryptographic crates remain forbid(unsafe_code).
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}
#[inline(always)]
fn wr(addr: u32, v: u32) {
    unsafe { core::ptr::write_volatile(addr as *mut u32, v) }
}

fn uart_init() {
    wr(SYSCTL_RCGCUART, rd(SYSCTL_RCGCUART) | 1);
    wr(SYSCTL_RCGCGPIO, rd(SYSCTL_RCGCGPIO) | 1); // port A
    for _ in 0..10_000 {
        let _ = rd(SYSCTL_RCGCGPIO);
    }
    // PA0/PA1 alternate function = UART
    wr(GPIOA_BASE + 0x420, 0x3); // AFSEL PA0,PA1
    wr(GPIOA_BASE + 0x52C, 0x11); // PCTL AF1
    wr(GPIOA_BASE + 0x51C, 0x3); // DEN PA0,PA1
    wr(UART0_CTL, 0);
    // 16 MHz, 115200 baud: BRD = 16e6/(16*115200) = 8.6805 -> IBRD=8, FBRD=44
    wr(UART0_IBRD, 8);
    wr(UART0_FBRD, 44);
    wr(UART0_LCRH, 0x70); // 8N1, FIFO
    wr(UART0_CC, 0);
    wr(UART0_CTL, 0x301); // UARTEN | TXE | RXE
}

struct Uart;
impl Write for Uart {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes() {
            while rd(UART0_FR) & (1 << 5) != 0 {} // TXFF
            wr(UART0_DR, b as u32);
        }
        Ok(())
    }
}

const KEY: [u8; 16] = [0x42; 16];
const NONCE: [u8; 16] = [0xAA; 16];
const AD: [u8; 8] = [0xAD; 8];
const SIZES: [usize; 7] = [8, 16, 32, 64, 128, 256, 512];
const WARMUP: u32 = 50;
const ITERS: u32 = 500;

#[inline(always)]
fn cyccnt() -> u32 {
    DWT::cycle_count()
}

#[entry]
fn main() -> ! {
    // Enable DWT cycle counter.
    let mut core = cortex_m::Peripherals::take().unwrap();
    core.DCB.enable_trace();
    core.DWT.enable_cycle_counter();

    uart_init();
    let mut u = Uart;

    let _ = writeln!(u, "# RustGuard TM4C123 perf benchmark (real DWT)");
    let _ = writeln!(u, "# 16 MHz, opt-level=3, lto=true. {} iters, {} warmup", ITERS, WARMUP);

    // Permutation micro-benchmarks.
    for (label, rounds) in [("p6", 6usize), ("p12", 12usize)] {
        let mut st = State { x0: 1, x1: 2, x2: 3, x3: 4, x4: 5 };
        for _ in 0..WARMUP {
            ascon_p(&mut st, rounds);
        }
        let start = cyccnt();
        for _ in 0..ITERS {
            ascon_p(&mut st, rounds);
        }
        let total = cyccnt().wrapping_sub(start);
        let _ = writeln!(u, "PERM {} mean_cyc={}", label, total / ITERS);
    }

    let mut pt = [0u8; 512];
    let mut ct = [0u8; 512];
    let mut rec = [0u8; 512];
    let mut tag = [0u8; 16];
    for (i, b) in pt.iter_mut().enumerate() {
        *b = i as u8;
    }

    let _ = writeln!(u, "SECTION:ENCRYPT");
    for &sz in &SIZES {
        for _ in 0..WARMUP {
            ascon_aead_encrypt(&KEY, &NONCE, &AD, &pt[..sz], &mut ct[..sz], &mut tag);
        }
        let start = cyccnt();
        for _ in 0..ITERS {
            ascon_aead_encrypt(&KEY, &NONCE, &AD, &pt[..sz], &mut ct[..sz], &mut tag);
        }
        let mean = cyccnt().wrapping_sub(start) / ITERS;
        let cpb = (mean as u64 * 100) / sz as u64;
        let _ = writeln!(u, "ENC {} mean_cyc={} cpb_x100={}", sz, mean, cpb);
    }

    let _ = writeln!(u, "SECTION:DECRYPT");
    for &sz in &SIZES {
        // produce a valid ct/tag for this size first
        ascon_aead_encrypt(&KEY, &NONCE, &AD, &pt[..sz], &mut ct[..sz], &mut tag);
        for _ in 0..WARMUP {
            let _ = ascon_aead_decrypt(&KEY, &NONCE, &AD, &ct[..sz], &mut rec[..sz], &tag);
        }
        let start = cyccnt();
        for _ in 0..ITERS {
            let _ = ascon_aead_decrypt(&KEY, &NONCE, &AD, &ct[..sz], &mut rec[..sz], &tag);
        }
        let mean = cyccnt().wrapping_sub(start) / ITERS;
        let _ = writeln!(u, "DEC {} mean_cyc={}", sz, mean);
    }

    let _ = writeln!(u, "SECTION:DONE");
    loop {
        cortex_m::asm::wfi();
    }
}
