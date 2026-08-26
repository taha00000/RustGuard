//! TM4C123GH6PM firmware — two modes, selected at build time.
//!
//!  * default build           : performance benchmark. Streams real ASCON-128
//!                              cycle counts (DWT CYCCNT) over UART0.
//!  * `--features timing`      : dudect-style timing-leakage harness. The host
//!                              drives fixed-vs-random inputs; the firmware
//!                              measures the cycle count of each crypto op with
//!                              interrupts disabled and returns it. No external
//!                              capture hardware is needed — the ARM core's own
//!                              cycle counter is the instrument.
//!  * `--features "timing leaky"` : as above, but the decrypt path links the
//!                              variable-time tag compare — the known-leaking
//!                              positive control that validates the method.
//!
//! Timing-leakage detection on-chip follows Reparaz, Balasch & Verbauwhede,
//! "Dude, is my code constant time?" (DATE 2017): collect execution-time samples
//! for two input classes and apply Welch's t-test (analysis/dudect.py). If the
//! constant-time guarantee holds through compilation, the two classes are
//! statistically indistinguishable (|t| < 4.5); the leaky control separates them.
//!
//! ## UART capture (read before flashing)
//! Do NOT rely on the ICDI virtual-COM port (the Windows driver conflict that
//! blocked the previous attempt). Wire an external USB-UART dongle (FT232/CP2102)
//! to PA1 (TX) -> dongle RX, GND -> GND, 115200 8N1. See docs/hardware_setup.md.

#![no_std]
#![no_main]

use core::fmt::Write;
use cortex_m::peripheral::DWT;
use cortex_m_rt::entry;
use panic_halt as _;

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
    // Isolated `unsafe` for MMIO only; the crypto crates stay forbid(unsafe_code).
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
    // PA0 (RX) / PA1 (TX) alternate function = UART0
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

#[inline(always)]
fn cyccnt() -> u32 {
    DWT::cycle_count()
}

#[entry]
fn main() -> ! {
    // Enable the DWT cycle counter (the measurement instrument for both modes).
    let mut core = cortex_m::Peripherals::take().unwrap();
    core.DCB.enable_trace();
    core.DWT.enable_cycle_counter();

    uart_init();
    let mut u = Uart;

    run(&mut u)
}

// ─────────────────────────────────────────────────────────────────────────────
// Mode A: performance benchmark (default build)
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(not(feature = "timing"))]
fn run(u: &mut Uart) -> ! {
    use rustguard_core::{ascon_aead_decrypt, ascon_aead_encrypt, ascon_p, State};

    const KEY: [u8; 16] = [0x42; 16];
    const NONCE: [u8; 16] = [0xAA; 16];
    const AD: [u8; 8] = [0xAD; 8];
    const SIZES: [usize; 7] = [8, 16, 32, 64, 128, 256, 512];
    const WARMUP: u32 = 50;
    const ITERS: u32 = 500;

    // Stream the whole benchmark repeatedly so a serial listener always catches
    // a full cycle without needing to reset the board at the right instant.
    loop {
    let _ = writeln!(u, "# RustGuard TM4C123 perf benchmark (real DWT)");
    let _ = writeln!(u, "# 16 MHz, opt-level=3, lto=true. {} iters, {} warmup", ITERS, WARMUP);

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
    // ~1 s gap, then stream the whole benchmark again.
    for _ in 0..6_000_000u32 {
        cortex_m::asm::nop();
    }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Mode B: dudect-style timing-leakage harness (`--features timing`)
// ─────────────────────────────────────────────────────────────────────────────
// Multi-primitive dispatch. One image carries every probe in the `probes`
// registry; the host selects one at runtime, so a whole ecosystem sweep needs a
// handful of flash cycles instead of one per crate.
//
// Protocol (ASCII, hex payloads):
//   l              list probes      -> "p <id> <taglen> <name>" lines, then "endp"
//   s<2 hex>       select probe     -> "ok <taglen>" | "err"
//   g              genuine tag      -> "tag <hex>"
//   v<taglen*2>    verify timing    -> "cyc <n>"
#[cfg(feature = "timing")]
fn run(u: &mut Uart) -> ! {
    use core::hint::black_box;
    use probes::{Probe, MAX_TAG, PROBES};

    fn hexval(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => 0,
        }
    }
    fn getc() -> u8 {
        while rd(UART0_FR) & (1 << 4) != 0 {} // RXFE (receive FIFO empty)
        rd(UART0_DR) as u8
    }
    fn read_hex(n: usize, out: &mut [u8]) {
        for byte in out.iter_mut().take(n) {
            let hi = hexval(getc());
            let lo = hexval(getc());
            *byte = (hi << 4) | lo;
        }
    }
    fn send_hex(u: &mut Uart, bytes: &[u8]) {
        const LUT: &[u8; 16] = b"0123456789abcdef";
        for &b in bytes {
            let _ = u.write_str(unsafe {
                core::str::from_utf8_unchecked(&[LUT[(b >> 4) as usize]])
            });
            let _ = u.write_str(unsafe {
                core::str::from_utf8_unchecked(&[LUT[(b & 0xf) as usize]])
            });
        }
    }

    /// Time the selected crate's own verification. Interrupts are masked and the
    /// tag is passed through `black_box` so the compiler cannot hoist or fold the
    /// comparison out of the measured region.
    fn measure_verify(p: &Probe, tag: &[u8]) -> u32 {
        cortex_m::interrupt::free(|_| {
            let s = cyccnt();
            let ok = (p.verify)(black_box(tag));
            let e = cyccnt();
            let _ = black_box(ok);
            e.wrapping_sub(s)
        })
    }

    let _ = writeln!(u, "# RustGuard multi-primitive timing harness (TM4C123)");
    let _ = writeln!(u, "# cmds: l=list s<id>=select g=get-tag v<tag>=verify-timing");
    let _ = writeln!(u, "# probes: {}", PROBES.len());
    let _ = writeln!(u, "READY");

    let mut sel: &'static Probe = &PROBES[0];

    loop {
        match getc() {
            b'l' => {
                for p in PROBES {
                    let _ = writeln!(u, "p {} {} {}", p.id, p.tag_len, p.name);
                }
                let _ = writeln!(u, "endp");
            }
            b's' => {
                let mut idb = [0u8; 1];
                read_hex(1, &mut idb);
                match probes::find(idb[0]) {
                    Some(p) => {
                        sel = p;
                        let _ = writeln!(u, "ok {}", p.tag_len);
                    }
                    None => {
                        let _ = writeln!(u, "err");
                    }
                }
            }
            b'g' => {
                let mut buf = [0u8; MAX_TAG];
                let n = (sel.correct_tag)(&mut buf);
                let _ = u.write_str("tag ");
                send_hex(u, &buf[..n]);
                let _ = writeln!(u);
            }
            b'v' => {
                let mut tag = [0u8; MAX_TAG];
                read_hex(sel.tag_len, &mut tag);
                let c = measure_verify(sel, &tag[..sel.tag_len]);
                let _ = writeln!(u, "cyc {}", c);
            }
            _ => {}
        }
    }
}
