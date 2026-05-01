#![no_std]
#![no_main]

//! rustguard-hal-tiva — RustGuard Benchmark Firmware for TM4C123GH6PM
//!
//! Board:  EK-TM4C123GXL (Tiva C LaunchPad)
//! MCU:    TM4C123GH6PM, ARM Cortex-M4F
//! Clock:  16 MHz (internal oscillator, default — no PLL)
//! UART:   UART0 on PA0/PA1, 9600 baud (direct register access)
//! Timing: DWT CYCCNT cycle counter (1-cycle resolution)
//!
//! Build:
//!   rustup target add thumbv7em-none-eabihf
//!   cargo build --release --target thumbv7em-none-eabihf
//!   rust-objcopy -O binary target/thumbv7em-none-eabihf/release/rustguard-hal-tiva out.bin
//!
//! Flash (LM Flash Programmer GUI or CLI):
//!   LMFlash.exe -i ICDI -v -r out.bin
//!
//! Serial output: COM port at 9600 baud, 8N1
//! LED sequence on boot: Red -> Blue -> Green (confirms firmware running)
//! LED on completion: cycling Red/Blue/Green

use core::fmt::Write;
use cortex_m::peripheral::DWT;
use cortex_m_rt::entry;
use panic_halt as _;

use rustguard_core::{ascon_aead_encrypt, ascon_aead_decrypt, ascon_p, State};
use rustguard_pap::PacketBuilder;

// ── Register addresses ────────────────────────────────────────────────────────
const SYSCTL_RCGCGPIO: u32 = 0x400FE608;
const SYSCTL_RCGCUART: u32 = 0x400FE618;
const GPIOA_BASE:      u32 = 0x40004000;
const GPIOF_BASE:      u32 = 0x40025000;
const UART0_DR:        u32 = 0x4000C000;
const UART0_FR:        u32 = 0x4000C018;
const UART0_IBRD:      u32 = 0x4000C024;
const UART0_FBRD:      u32 = 0x4000C028;
const UART0_LCRH:      u32 = 0x4000C02C;
const UART0_CTL:       u32 = 0x4000C030;
const UART0_CC:        u32 = 0x4000CFC8;

#[inline(always)]
unsafe fn rreg(a: u32) -> u32 { core::ptr::read_volatile(a as *const u32) }
#[inline(always)]
unsafe fn wreg(a: u32, v: u32) { core::ptr::write_volatile(a as *mut u32, v); }
#[inline(always)]
unsafe fn orreg(a: u32, b: u32) { wreg(a, rreg(a) | b); }

// ── UART0 init @ 9600 baud, 16 MHz clock ─────────────────────────────────────
// BRD = 16,000,000 / (16 * 9600) = 104.1666...
// IBRD = 104, FBRD = round(0.1666 * 64) = 11
unsafe fn uart0_init() {
    orreg(SYSCTL_RCGCGPIO, 1 << 0);
    orreg(SYSCTL_RCGCUART, 1 << 0);
    for _ in 0..10_000 { let _ = rreg(SYSCTL_RCGCUART); }

    orreg(GPIOA_BASE + 0x420, 0x03); // AFSEL: PA0,PA1
    wreg(GPIOA_BASE + 0x52C, 0x11);  // PCTL: AF1 (UART)
    orreg(GPIOA_BASE + 0x524, 0x03); // DEN
    wreg(GPIOA_BASE + 0x528, rreg(GPIOA_BASE + 0x528) & !0x03); // AMSEL clear

    wreg(UART0_CTL, 0x0000);
    for _ in 0..1_000 { let _ = rreg(UART0_FR); }
    wreg(UART0_CC,  0x0);   // system clock source
    wreg(UART0_IBRD, 104);
    wreg(UART0_FBRD, 11);
    wreg(UART0_LCRH, 0x70); // 8N1, FIFO enabled
    wreg(UART0_CTL, 0x0301);
    for _ in 0..50_000 { let _ = rreg(UART0_FR); }
}

// ── LED (PF1=Red, PF2=Blue, PF3=Green) ────────────────────────────────────────
unsafe fn led_init() {
    orreg(SYSCTL_RCGCGPIO, 1 << 5);
    for _ in 0..1_000 { let _ = rreg(SYSCTL_RCGCGPIO); }
    wreg(GPIOF_BASE + 0x520, 0x4C4F434B); // unlock
    wreg(GPIOF_BASE + 0x524, 0xFF);
    orreg(GPIOF_BASE + 0x400, 0x0E);      // PF1/PF2/PF3 output
    orreg(GPIOF_BASE + 0x51C, 0x0E);      // digital enable
}

unsafe fn led_color(r: bool, g: bool, b: bool) {
    let mut v = rreg(GPIOF_BASE + 0x3FC) & !0x0E;
    if r { v |= 0x02; }
    if g { v |= 0x08; }
    if b { v |= 0x04; }
    wreg(GPIOF_BASE + 0x3FC, v);
}

// ── UART writer ───────────────────────────────────────────────────────────────
struct Uart;
impl Write for Uart {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() {
            unsafe {
                if byte == b'\n' {
                    while rreg(UART0_FR) & (1 << 5) != 0 {}
                    wreg(UART0_DR, b'\r' as u32);
                }
                while rreg(UART0_FR) & (1 << 5) != 0 {}
                wreg(UART0_DR, byte as u32);
            }
        }
        Ok(())
    }
}

// ── Benchmark config ──────────────────────────────────────────────────────────
const WARMUP: u32 = 50;
const ITERS:  u32 = 500;
const KEY:    [u8; 16] = [0x42u8; 16];
const NONCE:  [u8; 16] = [0xAAu8; 16];
const AD:     [u8;  8] = [0x01u8;  8];

// 16 MHz: 1 cycle = 62.5 ns
fn ns(cycles: u32) -> u64 { cycles as u64 * 625 / 10 }

// ── Benchmark functions ───────────────────────────────────────────────────────
fn bench_enc(sz: usize) -> (u32, u32, u32) {
    let p = [0xBEu8; 512];
    let mut ct = [0u8; 512];
    let mut tag = [0u8; 16];
    for _ in 0..WARMUP {
        ascon_aead_encrypt(&KEY, &NONCE, &AD, &p[..sz], &mut ct[..sz], &mut tag);
    }
    let (mut sum, mut mn, mut mx) = (0u64, u32::MAX, 0u32);
    for _ in 0..ITERS {
        let t = DWT::cycle_count();
        ascon_aead_encrypt(&KEY, &NONCE, &AD, &p[..sz], &mut ct[..sz], &mut tag);
        let e = DWT::cycle_count().wrapping_sub(t);
        sum += e as u64; if e < mn { mn = e; } if e > mx { mx = e; }
    }
    ((sum / ITERS as u64) as u32, mn, mx)
}

fn bench_dec(sz: usize) -> (u32, u32, u32) {
    let p = [0xBEu8; 512];
    let mut ct = [0u8; 512];
    let mut pt = [0u8; 512];
    let mut tag = [0u8; 16];
    ascon_aead_encrypt(&KEY, &NONCE, &AD, &p[..sz], &mut ct[..sz], &mut tag);
    for _ in 0..WARMUP {
        ascon_aead_decrypt(&KEY, &NONCE, &AD, &ct[..sz], &mut pt[..sz], &tag);
    }
    let (mut sum, mut mn, mut mx) = (0u64, u32::MAX, 0u32);
    for _ in 0..ITERS {
        let t = DWT::cycle_count();
        ascon_aead_decrypt(&KEY, &NONCE, &AD, &ct[..sz], &mut pt[..sz], &tag);
        let e = DWT::cycle_count().wrapping_sub(t);
        sum += e as u64; if e < mn { mn = e; } if e > mx { mx = e; }
    }
    ((sum / ITERS as u64) as u32, mn, mx)
}

fn bench_pap(sz: usize) -> (u32, u32, u32) {
    let p = [0xBEu8; 512];
    let mut b = PacketBuilder::new(KEY, 0);
    for _ in 0..WARMUP { let _ = b.build_packet(&p[..sz], 0x0001, 1, 1); }
    let (mut sum, mut mn, mut mx) = (0u64, u32::MAX, 0u32);
    for _ in 0..ITERS {
        let t = DWT::cycle_count();
        let _ = b.build_packet(&p[..sz], 0x0001, 1, 1);
        let e = DWT::cycle_count().wrapping_sub(t);
        sum += e as u64; if e < mn { mn = e; } if e > mx { mx = e; }
    }
    ((sum / ITERS as u64) as u32, mn, mx)
}

fn bench_perm(rounds: usize) -> (u32, u32, u32) {
    let mut st = State { x0: 1, x1: 2, x2: 3, x3: 4, x4: 5 };
    for _ in 0..WARMUP { ascon_p(&mut st, rounds); }
    let (mut sum, mut mn, mut mx) = (0u64, u32::MAX, 0u32);
    for _ in 0..ITERS {
        let t = DWT::cycle_count();
        ascon_p(&mut st, rounds);
        let e = DWT::cycle_count().wrapping_sub(t);
        sum += e as u64; if e < mn { mn = e; } if e > mx { mx = e; }
    }
    ((sum / ITERS as u64) as u32, mn, mx)
}

// ── Self-test ─────────────────────────────────────────────────────────────────
fn self_test() -> bool {
    let pt = b"RustGuard ASCON-128 self-test 32";
    let mut ct  = [0u8; 32];
    let mut tag = [0u8; 16];
    let mut rec = [0u8; 32];
    ascon_aead_encrypt(&KEY, &NONCE, b"ad", pt, &mut ct, &mut tag);
    if !ascon_aead_decrypt(&KEY, &NONCE, b"ad", &ct, &mut rec, &tag) { return false; }
    if &rec != pt { return false; }
    ct[0] ^= 1;
    if ascon_aead_decrypt(&KEY, &NONCE, b"ad", &ct, &mut rec, &tag) { return false; }
    let mut builder = PacketBuilder::new(KEY, 0);
    let pkt = builder.build_packet(b"test payload ok!", 0x0001, 1, 1);
    let rx  = PacketBuilder::new(KEY, 0);
    let mut out = [0u8; 64];
    rx.unwrap_packet(&pkt, 0, &mut out).is_ok()
}

// ── Entry ─────────────────────────────────────────────────────────────────────
#[entry]
fn main() -> ! {
    unsafe { led_init(); uart0_init(); }

    let mut cp = unsafe { cortex_m::Peripherals::steal() };
    cp.DCB.enable_trace();
    cp.DWT.enable_cycle_counter();

    let mut u = Uart;

    // Boot sequence: Red -> Blue -> Green
    unsafe {
        led_color(true, false, false);
        cortex_m::asm::delay(3_200_000);
        led_color(false, false, true);
        cortex_m::asm::delay(3_200_000);
        led_color(false, true, false);
        cortex_m::asm::delay(3_200_000);
    }

    writeln!(u, "").ok();
    writeln!(u, "RustGuard TM4C123GH6PM @ 16MHz").ok();
    writeln!(u, "Iters={ITERS} Warmup={WARMUP} | 1cyc=62.5ns").ok();
    writeln!(u, "").ok();

    writeln!(u, "SECTION:SELF_TEST").ok();
    if self_test() {
        writeln!(u, "SELF_TEST PASS").ok();
        unsafe { led_color(false, true, false); }
    } else {
        writeln!(u, "SELF_TEST FAIL").ok();
        unsafe { led_color(true, false, false); }
        loop {}
    }
    writeln!(u, "").ok();

    writeln!(u, "SECTION:PERMUTATION").ok();
    let (m6,  n6,  x6)  = bench_perm(6);
    let (m12, n12, x12) = bench_perm(12);
    writeln!(u, "PERM p6  mean={m6} min={n6} max={x6} ns={}", ns(m6)).ok();
    writeln!(u, "PERM p12 mean={m12} min={n12} max={x12} ns={}", ns(m12)).ok();
    writeln!(u, "").ok();

    writeln!(u, "SECTION:ENCRYPT_LATENCY").ok();
    for sz in [8usize, 16, 32, 64, 128, 256, 512] {
        let (m, n, x) = bench_enc(sz);
        let cpb = m as u64 * 100 / sz as u64;
        writeln!(u, "ENC_LAT {sz:3} mean={m} min={n} max={x} ns={} cpb={}.{:02}",
                 ns(m), cpb / 100, cpb % 100).ok();
    }
    writeln!(u, "").ok();

    writeln!(u, "SECTION:DECRYPT_LATENCY").ok();
    for sz in [8usize, 16, 32, 64, 128, 256, 512] {
        let (m, n, x) = bench_dec(sz);
        writeln!(u, "DEC_LAT {sz:3} mean={m} min={n} max={x} ns={}", ns(m)).ok();
    }
    writeln!(u, "").ok();

    writeln!(u, "SECTION:PAP_LATENCY").ok();
    for sz in [8usize, 16, 32, 64, 128, 256, 512] {
        let (m, n, x) = bench_pap(sz);
        writeln!(u, "PAP_LAT {sz:3} mean={m} min={n} max={x} ns={}", ns(m)).ok();
    }
    writeln!(u, "").ok();

    writeln!(u, "SECTION:COMPLETE").ok();
    writeln!(u, "Copy above output to results/raw/benchmark_tm4c.txt").ok();
    writeln!(u, "Then run: python3 scripts/parse_hw_results.py results/raw/benchmark_tm4c.txt").ok();

    // Completion: cycle all three colors
    loop {
        unsafe {
            led_color(true, false, false);
            cortex_m::asm::delay(800_000);
            led_color(false, false, true);
            cortex_m::asm::delay(800_000);
            led_color(false, true, false);
            cortex_m::asm::delay(800_000);
        }
    }
}
