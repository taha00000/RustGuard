//! STM32F303 (STM32F3 Discovery) dudect-style timing-leakage harness.
//!
//! The cross-silicon counterpart to `firmware-tm4c` (a *different vendor's*
//! Cortex-M4). The protocol, measurement, and analysis are identical — only the
//! UART/clock bring-up is STM32-specific — so the same `capture/collect_timing.py`
//! and `analysis/dudect.py` drive it. Running the same experiment here shows the
//! constant-time result is not an artifact of one microarchitecture.
//!
//! ## Protocol (same as firmware-tm4c; one ASCII line per reply)
//!   'l'            list probes      -> "p <id> <taglen> <name>" ... "endp"
//!   's' <id:2hex>  select a probe   -> "ok <taglen>" | "err"
//!   'g'            correct tag      -> "tag <hex>"
//!   'v' <tag hex>  time verify      -> "cyc <n>"
//! Build:  default = all registry probes ; `--features leaky` adds the control.
//!
//! ## Serial: two routes, served simultaneously by the same image
//!   USART1 PC4 (TX) / PC5 (RX) -> ST-LINK/V2-B virtual COM port on board
//!                                 revisions that route it (no wiring at all);
//!   USART2 PA2 (TX) / PA3 (RX) -> external USB-UART dongle
//!                                 (PA2 -> dongle RX, PA3 -> dongle TX, GND-GND).
//! Output goes to both; input is accepted from whichever receives it.
//! 115200 8N1 on the internal HSI (8 MHz), so no external clock is needed.

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
    const RCC_APB2ENR: u32 = 0x4002_1018; // USART1 clock
    const RCC_APB1ENR: u32 = 0x4002_101C; // USART2 clock
    const RCC_AHBENR_IOPAEN: u32 = 1 << 17;
    const RCC_AHBENR_IOPCEN: u32 = 1 << 19;
    const RCC_APB2ENR_USART1EN: u32 = 1 << 14;
    const RCC_APB1ENR_USART2EN: u32 = 1 << 17;

    const GPIOA: u32 = 0x4800_0000;
    const GPIOC: u32 = 0x4800_0800;
    const MODER: u32 = 0x00;
    const PUPDR: u32 = 0x0C;
    const AFRL: u32 = 0x20;

    const USART1: u32 = 0x4001_3800;
    const USART2: u32 = 0x4000_4400;
    const CR1: u32 = 0x00;
    const BRR: u32 = 0x0C;
    const ISR: u32 = 0x1C;
    const ICR: u32 = 0x20;
    const RDR: u32 = 0x24;
    const TDR: u32 = 0x28;
    const ISR_ERR: u32 = 0b1111; // PE | FE | NE | ORE
    const ISR_RXNE: u32 = 1 << 5;
    const ISR_TXE: u32 = 1 << 7;
    const CR1_UE: u32 = 1 << 0;
    const CR1_RE: u32 = 1 << 2;
    const CR1_TE: u32 = 1 << 3;

    // HSI = 8 MHz after reset; PCLK1 = PCLK2 = 8 MHz (prescalers /1). 115200 8N1.
    const CLOCK_HZ: u32 = 8_000_000;
    const BAUD: u32 = 115_200;

    const PORTS: [u32; 2] = [USART1, USART2];

    /// Next byte from whichever USART has one. Framing/noise/overrun errors are
    /// cleared and the byte dropped, so an unconnected route cannot inject junk.
    pub fn getc() -> u8 {
        loop {
            for &u in PORTS.iter() {
                let isr = rd(u + ISR);
                if isr & ISR_ERR != 0 {
                    wr(u + ICR, ISR_ERR);
                    let _ = rd(u + RDR);
                    continue;
                }
                if isr & ISR_RXNE != 0 {
                    return rd(u + RDR) as u8;
                }
            }
        }
    }
    pub fn putc(b: u8) {
        for &u in PORTS.iter() {
            while rd(u + ISR) & ISR_TXE == 0 {}
            wr(u + TDR, b as u32);
        }
    }

    /// Put two pins of `port` into AF7 with a pull-up on the RX pin.
    fn af7(port: u32, tx: u32, rx: u32) {
        let mut m = rd(port + MODER);
        m &= !((0b11 << (2 * tx)) | (0b11 << (2 * rx)));
        m |= (0b10 << (2 * tx)) | (0b10 << (2 * rx));
        wr(port + MODER, m);
        let mut p = rd(port + PUPDR);
        p &= !(0b11 << (2 * rx));
        p |= 0b01 << (2 * rx); // idle-high when nothing is attached
        wr(port + PUPDR, p);
        let mut a = rd(port + AFRL);
        a &= !((0xF << (4 * tx)) | (0xF << (4 * rx)));
        a |= (7 << (4 * tx)) | (7 << (4 * rx));
        wr(port + AFRL, a);
    }

    /// USART1 on PC4/PC5 (ST-LINK VCP route) and USART2 on PA2/PA3 (dongle route).
    pub fn uart_init() {
        wr(RCC_AHBENR, rd(RCC_AHBENR) | RCC_AHBENR_IOPAEN | RCC_AHBENR_IOPCEN);
        wr(RCC_APB2ENR, rd(RCC_APB2ENR) | RCC_APB2ENR_USART1EN);
        wr(RCC_APB1ENR, rd(RCC_APB1ENR) | RCC_APB1ENR_USART2EN);
        af7(GPIOC, 4, 5);
        af7(GPIOA, 2, 3);
        for &u in PORTS.iter() {
            wr(u + BRR, CLOCK_HZ / BAUD); // integer oversampling-by-16 divisor
            wr(u + CR1, CR1_UE | CR1_TE | CR1_RE);
        }
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
