//! STM32F303 (STM32F3 Discovery) dudect-style timing-leakage harness.
//!
//! The cross-silicon counterpart to `firmware-tm4c` (a *different vendor's*
//! Cortex-M4). The protocol, measurement, and analysis are identical — only the
//! UART/clock bring-up is STM32-specific — so the same `capture/collect_timing.py`
//! and `analysis/dudect.py` drive it. Running the same experiment here shows the
//! constant-time result is not an artifact of one microarchitecture.
//!
//! ## Protocol (one ASCII command, hex-encoded; reply on the same UART)
//!   'e' <key><pt>       measure encrypt cycles              -> "cyc <n>"
//!   'v' <key><tag>      measure decrypt cycles (tag check)  -> "cyc <n>"
//!   'g' <key>           report the correct tag              -> "tag <hex>"
//! Build:  default = constant-time DUT ; `--features leaky` = variable-time control.
//!
//! ## Wiring (the F3 Discovery's ST-LINK has no serial port, so use a dongle)
//!   USART2 TX = PA2 -> dongle RX
//!   USART2 RX = PA3 -> dongle TX
//!   GND             -> dongle GND    (115200 8N1)
//! Clock: internal HSI (8 MHz), so no external clock is needed to bring up UART.

#![no_std]
#![no_main]

use core::fmt::Write;
use cortex_m::peripheral::DWT;
use cortex_m_rt::entry;
use panic_halt as _;
use probes::{Probe, MAX_TAG, PROBES};

mod board {
    //! Isolated `unsafe` MMIO for the STM32F303 (RM0316). The crypto crates stay
    //! `forbid(unsafe_code)`.
    #[inline(always)]
    pub fn rd(a: u32) -> u32 {
        unsafe { core::ptr::read_volatile(a as *const u32) }
    }
    #[inline(always)]
    pub fn wr(a: u32, v: u32) {
        unsafe { core::ptr::write_volatile(a as *mut u32, v) }
    }

    // RM0316 register map.
    const RCC_AHBENR: u32 = 0x4002_1014; // GPIO port clocks
    const RCC_APB1ENR: u32 = 0x4002_101C; // USART2 clock
    const RCC_AHBENR_IOPAEN: u32 = 1 << 17;
    const RCC_APB1ENR_USART2EN: u32 = 1 << 17;

    const GPIOA_MODER: u32 = 0x4800_0000;
    const GPIOA_AFRL: u32 = 0x4800_0020;

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

    // HSI = 8 MHz after reset; PCLK1 = 8 MHz (prescalers /1). 115200 8N1.
    const CLOCK_HZ: u32 = 8_000_000;
    const BAUD: u32 = 115_200;

    pub fn getc() -> u8 {
        while rd(USART_ISR) & USART_ISR_RXNE == 0 {}
        rd(USART_RDR) as u8
    }
    pub fn putc(b: u8) {
        while rd(USART_ISR) & USART_ISR_TXE == 0 {}
        wr(USART_TDR, b as u32);
    }

    /// Bring up GPIOA + USART2 on PA2 (TX) / PA3 (RX), AF7, 115200 8N1, on HSI.
    pub fn uart_init() {
        wr(RCC_AHBENR, rd(RCC_AHBENR) | RCC_AHBENR_IOPAEN);
        wr(RCC_APB1ENR, rd(RCC_APB1ENR) | RCC_APB1ENR_USART2EN);

        // PA2, PA3 -> alternate function (0b10).
        let mut moder = rd(GPIOA_MODER);
        moder &= !((0b11 << 4) | (0b11 << 6));
        moder |= (0b10 << 4) | (0b10 << 6);
        wr(GPIOA_MODER, moder);

        // PA2, PA3 -> AF7 (USART2).
        let mut afrl = rd(GPIOA_AFRL);
        afrl &= !((0xF << 8) | (0xF << 12));
        afrl |= (7 << 8) | (7 << 12);
        wr(GPIOA_AFRL, afrl);

        wr(USART2_BRR, CLOCK_HZ / BAUD); // integer oversampling-by-16 divisor
        wr(USART2_CR1, USART_CR1_UE | USART_CR1_TE | USART_CR1_RE);
    }
}

struct Uart;
impl Write for Uart {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes() {
            board::putc(b);
        }
        Ok(())
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
fn read_hex(n: usize, out: &mut [u8]) {
    for byte in out.iter_mut().take(n) {
        let hi = hexval(board::getc());
        let lo = hexval(board::getc());
        *byte = (hi << 4) | lo;
    }
}
fn send_hex(bytes: &[u8]) {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    for &b in bytes {
        board::putc(LUT[(b >> 4) as usize]);
        board::putc(LUT[(b & 0xf) as usize]);
    }
}

#[inline(always)]
fn cyccnt() -> u32 {
    DWT::cycle_count()
}

/// Time the selected crate's own verification, interrupts masked, with the tag
/// behind `black_box` so the comparison cannot be hoisted out of the measured
/// region. Identical to the TM4C harness — that is the point: same measurement,
/// different silicon vendor.
fn measure_verify(p: &Probe, tag: &[u8]) -> u32 {
    use core::hint::black_box;
    cortex_m::interrupt::free(|_| {
        let s = cyccnt();
        let ok = (p.verify)(black_box(tag));
        let e = cyccnt();
        let _ = black_box(ok);
        e.wrapping_sub(s)
    })
}

#[entry]
fn main() -> ! {
    let mut core = cortex_m::Peripherals::take().unwrap();
    core.DCB.enable_trace();
    core.DWT.enable_cycle_counter();

    board::uart_init();
    let mut u = Uart;

    let _ = writeln!(u, "# RustGuard multi-primitive timing harness (STM32F303)");
    let _ = writeln!(u, "# cmds: l=list s<id>=select g=get-tag v<tag>=verify-timing");
    let _ = writeln!(u, "# probes: {}", PROBES.len());
    let _ = writeln!(u, "READY");

    let mut sel: &'static Probe = &PROBES[0];

    loop {
        match board::getc() {
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
                send_hex(&buf[..n]);
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
